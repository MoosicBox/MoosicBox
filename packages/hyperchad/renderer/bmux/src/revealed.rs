//! Viewport route lifecycle. Geometry comes from the resolved layout, never document
//! coordinates or the terminal alone. Failures rearm only after leaving the band
//! (or an explicit click), so a stationary failed sentinel cannot hot-loop.

use std::collections::{BTreeMap, BTreeSet};

use hyperchad_transformer::Container;
use hyperchad_transformer_models::Route;

use super::{BmuxProgram, visit};
use crate::{
    actions::EventKind,
    layout,
    renderer::{Error, HostResponse},
};

const REQUEST_PREFIX: &str = "bmux:revealed:";

#[derive(Default)]
pub(super) struct RevealedRoutes {
    entries: BTreeMap<String, Entry>,
    serial: u64,
    // Rebuilt alongside full layout from the effective responsive document.
    // Scroll reprojection and paint-only prepares visit only these route keys.
    candidates: Vec<(String, Route)>,
    chains: BTreeMap<u64, Chain>,
    chain_keys: BTreeMap<String, u64>,
    next_chain: u64,
    active_chain: Option<u64>,
    continuation_routes: Vec<Route>,
}

#[derive(Default)]
struct Chain {
    admissions: u8,
    motion: Option<Motion>,
}

#[derive(PartialEq, Eq)]
struct Motion {
    viewport: bmux_tui::geometry::Size,
    offsets: Vec<(String, layout::ScrollOffset)>,
}

fn projected_node<'a>(
    tree: &'a layout::LayoutTree,
    projection: Option<&layout::ViewportProjection>,
    key: &str,
) -> Option<std::borrow::Cow<'a, layout::ResolvedNode>> {
    projection.map_or_else(
        || tree.get(key).map(std::borrow::Cow::Borrowed),
        |projection| projection.geometry_node(key).map(std::borrow::Cow::Owned),
    )
}

impl Chain {
    fn observe_motion(
        &mut self,
        node: &layout::ResolvedNode,
        tree: &layout::LayoutTree,
        projection: Option<&layout::ViewportProjection>,
    ) {
        let mut offsets = Vec::new();
        let mut parent = node.parent_key.clone();
        while let Some(ancestor) = parent
            .as_deref()
            .and_then(|key| projected_node(tree, projection, key))
        {
            if ancestor.scroll_offset != layout::ScrollOffset::default() {
                offsets.push((ancestor.key.clone(), ancestor.scroll_offset));
            }
            parent.clone_from(&ancestor.parent_key);
        }
        let motion = Motion {
            viewport: tree.viewport,
            offsets,
        };
        if self.motion.as_ref() != Some(&motion) {
            self.admissions = 0;
            self.motion = Some(motion);
        }
    }
}

struct Entry {
    route: Route,
    epoch: u64,
    near: bool,
    phase: Phase,
}

#[derive(Default)]
enum Phase {
    #[default]
    Ready,
    Pending {
        id: String,
        generation: u64,
    },
    Failed,
    Done,
}

fn revealed(route: &Route) -> bool {
    matches!(route, Route::Get { trigger, .. } if trigger.as_deref() == Some("revealed"))
}

fn near_viewport(bounds: layout::CellRect, clip: layout::CellRect) -> bool {
    // At most eight terminal rows, and no more than half the resolved ancestor
    // viewport. Do not expand horizontally or revive an empty ancestor clip.
    if bounds.width == 0 || bounds.height == 0 || clip.width == 0 || clip.height == 0 {
        return false;
    }
    let margin = i64::try_from((clip.height / 2).min(8)).unwrap_or(8);
    bounds.x < clip.right()
        && bounds.right() > clip.x
        && bounds.y < clip.bottom().saturating_add(margin)
        && bounds.bottom() > clip.y.saturating_sub(margin)
}

impl RevealedRoutes {
    pub(super) fn reset_burst(&mut self, key: &str) {
        if let Some(chain) = self
            .chain_keys
            .get(key)
            .and_then(|id| self.chains.get_mut(id))
        {
            chain.admissions = 0;
        }
    }

    pub(super) fn register_chain(&mut self, node: &Container, path: &[usize]) {
        if !node.route.as_ref().is_some_and(revealed) {
            return;
        }
        let key = layout::node_key(node, path);
        if self.chain_keys.contains_key(&key) {
            return;
        }
        let inherited = self.active_chain.filter(|_| {
            self.continuation_routes
                .iter()
                .any(|route| node.route.as_ref() == Some(route))
        });
        let id = inherited.unwrap_or_else(|| {
            self.next_chain = self.next_chain.saturating_add(1);
            self.next_chain
        });
        self.chain_keys.insert(key, id);
        self.chains.entry(id).or_default();
    }

    pub(super) fn refresh_candidates(&mut self, root: &Container) {
        self.candidates.clear();
        visit(root, &mut Vec::new(), &mut |node, path| {
            if let Some(route) = node.route.as_ref().filter(|route| revealed(route)) {
                self.candidates
                    .push((layout::node_key(node, path), route.clone()));
            }
        });
        let live: BTreeSet<_> = self.chain_keys.values().copied().collect();
        self.chains.retain(|id, _| live.contains(id));
    }

    pub(super) fn remove(&mut self, key: &str) {
        self.entries.remove(key);
        self.chain_keys.remove(key);
    }

    pub(super) fn clear(&mut self) {
        // Keep the candidate index and serial: context invalidation is not a
        // document replacement, and old completions must never regain ownership.
        self.entries.clear();
        for chain in self.chains.values_mut() {
            chain.admissions = 0;
        }
    }

    pub(super) fn reset_document(&mut self) {
        self.clear();
        self.chain_keys.clear();
        self.chains.clear();
        self.candidates.clear();
    }

    fn entry(&mut self, key: &str, route: &Route, epoch: u64) -> &mut Entry {
        let entry = self.entries.entry(key.to_owned()).or_insert_with(|| Entry {
            route: route.clone(),
            epoch,
            near: false,
            phase: Phase::Ready,
        });
        if entry.route != *route || entry.epoch != epoch {
            *entry = Entry {
                route: route.clone(),
                epoch,
                near: false,
                phase: Phase::Ready,
            };
        }
        entry
    }

    fn observe(&mut self, key: &str, route: &Route, epoch: u64, near: bool) -> bool {
        let entry = self.entry(key, route, epoch);
        let entered = near && !entry.near;
        entry.near = near;
        near && (matches!(entry.phase, Phase::Ready)
            || (entered && matches!(entry.phase, Phase::Failed)))
    }

    fn admit(
        &mut self,
        request: &mut crate::actions::RequestEffect,
        route: &Route,
        epoch: u64,
    ) -> bool {
        if matches!(
            self.entry(&request.source.key, route, epoch).phase,
            Phase::Pending { .. } | Phase::Done
        ) {
            return false;
        }
        let Some(serial) = self.serial.checked_add(1) else {
            return false;
        };
        self.serial = serial;
        let id = format!("{REQUEST_PREFIX}{serial}");
        self.entry(&request.source.key, route, epoch).phase = Phase::Pending {
            id: id.clone(),
            generation: request.source.generation,
        };
        request.request_id = Some(id);
        true
    }

    pub(super) fn owns(request: &crate::actions::RequestEffect) -> bool {
        request
            .request_id
            .as_deref()
            .is_some_and(|id| id.starts_with(REQUEST_PREFIX))
    }

    fn current(
        &self,
        request: &crate::actions::RequestEffect,
        route: Option<&Route>,
        epoch: Option<u64>,
    ) -> bool {
        self.entries.get(&request.source.key).is_some_and(|entry| {
            route == Some(&entry.route) && epoch == Some(entry.epoch)
                && matches!(&entry.route, Route::Get { route, .. } if route == &request.url)
                && request.method == "GET"
                && matches!(&entry.phase, Phase::Pending { id, generation }
                    if Some(id) == request.request_id.as_ref() && *generation == request.source.generation)
        })
    }

    pub(super) fn finish(&mut self, key: &str, success: bool) {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.phase = if success { Phase::Done } else { Phase::Failed };
        }
    }
}

impl BmuxProgram {
    pub(super) fn schedule_revealed(&mut self) {
        let Some(tree) = self.candidate.take() else {
            return;
        };
        let started = std::time::Instant::now();
        let projection = self.candidate_projection.clone();
        self.schedule_revealed_in(&tree, projection.as_ref());
        self.performance.reveal_us += super::elapsed_us(started);
        self.candidate = Some(tree);
    }

    fn schedule_revealed_in(
        &mut self,
        tree: &layout::LayoutTree,
        projection: Option<&layout::ViewportProjection>,
    ) {
        let mut ready = Vec::new();
        let candidates = std::mem::take(&mut self.revealed_routes.candidates);
        for (key, route) in &candidates {
            self.performance.reveal_visited += 1;
            let Some(node) = projected_node(tree, projection, key) else {
                if let Some(entry) = self.revealed_routes.entries.get_mut(key) {
                    entry.near = false;
                }
                continue;
            };
            let Some(chain_id) = self.revealed_routes.chain_keys.get(key).copied() else {
                continue;
            };
            let chain = self.revealed_routes.chains.entry(chain_id).or_default();
            chain.observe_motion(&node, tree, projection);
            if chain.admissions >= 4 {
                continue;
            }
            let epoch = self
                .node_epochs
                .get(&node.key)
                .copied()
                .unwrap_or_else(|| self.state.generation());
            let mut visible = node.visible;
            let mut parent = node.parent_key.clone();
            while let Some(ancestor) = parent
                .as_deref()
                .and_then(|key| projected_node(tree, projection, key))
            {
                visible &= ancestor.visible;
                parent.clone_from(&ancestor.parent_key);
            }
            if self.revealed_routes.observe(
                &node.key,
                route,
                epoch,
                visible && near_viewport(node.bounds, node.clip),
            ) {
                ready.push((node.key.clone(), chain_id));
            }
        }
        self.revealed_routes.candidates = candidates;
        for (key, chain_id) in ready {
            if self.revealed_routes.chains[&chain_id].admissions >= 4 {
                continue;
            }
            let serial = self.revealed_routes.serial;
            if let Err(error) = self.dispatch_with_geometry(
                &key,
                EventKind::Named("revealed".into()),
                None,
                Some(tree),
            ) {
                self.revealed_routes.finish(&key, false);
                log::warn!("revealed route admission failed: {error}");
            }
            if self.revealed_routes.serial != serial {
                self.revealed_routes
                    .chains
                    .get_mut(&chain_id)
                    .unwrap()
                    .admissions += 1;
            }
        }
    }

    pub(super) fn admit_revealed(&mut self, request: &mut crate::actions::RequestEffect) -> bool {
        let Some(route) = self
            .state
            .find_node(&request.source.key)
            .and_then(|node| node.route.as_ref())
            .filter(|route| revealed(route))
        else {
            return true;
        };
        let epoch = *self
            .node_epochs
            .entry(request.source.key.clone())
            .or_insert(request.source.generation);
        self.revealed_routes.admit(request, route, epoch)
    }

    pub(super) fn revealed_current(&self, request: &crate::actions::RequestEffect) -> bool {
        !RevealedRoutes::owns(request)
            || self.revealed_routes.current(
                request,
                self.state
                    .find_node(&request.source.key)
                    .and_then(|node| node.route.as_ref()),
                self.node_epochs.get(&request.source.key).copied(),
            )
    }

    pub(super) fn complete_revealed(
        &mut self,
        request: &crate::actions::RequestEffect,
        result: Result<HostResponse, Error>,
    ) {
        let result = match result {
            Ok(HostResponse::View(view)) => {
                self.revealed_routes.active_chain = self
                    .revealed_routes
                    .chain_keys
                    .get(&request.source.key)
                    .copied();
                self.revealed_routes.continuation_routes.clear();
                if let Some(primary) = &view.primary {
                    visit(primary, &mut Vec::new(), &mut |node, _| {
                        if let Some(route) = node.route.as_ref().filter(|route| revealed(route)) {
                            self.revealed_routes.continuation_routes.push(route.clone());
                        }
                    });
                }
                let result = self.apply_request_view(request, &view);
                self.revealed_routes.active_chain = None;
                self.revealed_routes.continuation_routes.clear();
                result
            }
            Ok(HostResponse::Complete) => Ok(()),
            Ok(_) => Err(std::io::Error::other("unexpected revealed route response").into()),
            Err(error) => Err(error),
        };
        // This replacement may already have removed/replaced this identity. Never
        // mark a new sentinel done using the old sentinel's completion.
        if self.revealed_current(request) {
            self.revealed_routes
                .finish(&request.source.key, result.is_ok());
        }
        if let Err(error) = result {
            log::warn!("revealed route failed (click or leave/re-enter to retry): {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::near_viewport;
    use crate::{
        actions::EventKind,
        layout,
        program::{BmuxProgram, Completion, Work},
        renderer::{BmuxConfig, HostResponse},
    };
    use bmux_tui::geometry::Size;
    use hyperchad_renderer::View;
    use hyperchad_transformer::{Container, Number};
    use hyperchad_transformer_models::{
        LayoutDirection, LayoutOverflow, Route, Selector, SwapStrategy,
    };
    use std::{collections::BTreeMap, sync::Arc};

    fn sentinel(url: &str) -> Container {
        Container {
            width: Some(Number::Integer(160)),
            height: Some(Number::Integer(32)),
            route: Some(Route::Get {
                route: url.into(),
                trigger: Some("revealed".into()),
                target: Selector::SelfTarget,
                strategy: SwapStrategy::This,
            }),
            ..Default::default()
        }
    }

    fn program() -> BmuxProgram {
        let (actions, _) = flume::bounded(8);
        let (resize, _) = flume::bounded(8);
        BmuxProgram::new(BmuxConfig::default(), actions, resize)
    }

    fn document() -> Container {
        Container {
            direction: LayoutDirection::Column,
            children: vec![
                sentinel("/page/2"),
                Container {
                    str_id: Some("sibling".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn take_read(program: &mut BmuxProgram) -> (u64, crate::actions::RequestEffect) {
        match program.pending.pop_front().expect("one revealed request") {
            Work::Host {
                generation,
                completion: Completion::Read(request),
                ..
            } => (generation, request),
            _ => panic!("expected read lane"),
        }
    }

    fn position(program: &mut BmuxProgram, y: i64) {
        let node = program
            .candidate
            .as_mut()
            .map(Arc::make_mut)
            .unwrap()
            .nodes
            .iter_mut()
            .find(|node| node.key == "@0")
            .unwrap();
        node.bounds = layout::CellRect::new(0, y, 20, 2);
        node.clip = layout::CellRect::new(0, 0, 80, 24);
    }

    #[test]
    fn revealed_offscreen_near_dedupe_and_failure_retry() {
        let mut program = program();
        program.apply_view(View::from(document()), None).unwrap();
        assert!(
            program.pending.is_empty(),
            "insertion must not eagerly load"
        );
        // Prepare once to obtain real resolved geometry; discard its first admission
        // by starting a fresh sentinel lifecycle before testing offscreen admission.
        program.prepare().unwrap();
        program.pending.clear();
        program.revealed_routes.clear();
        position(&mut program, 200);
        program.schedule_revealed();
        assert!(program.pending.is_empty());
        position(&mut program, 30);
        program.schedule_revealed();
        let (epoch, request) = take_read(&mut program);
        program.schedule_revealed();
        program.dispatch("@0", EventKind::Click, None).unwrap();
        assert!(
            program.pending.is_empty(),
            "pending reveal and click dedupe"
        );
        program
            .complete(
                epoch,
                Completion::Read(request),
                Err(std::io::Error::other("offline").into()),
            )
            .unwrap();
        for _ in 0..3 {
            program.schedule_revealed();
        }
        assert!(program.pending.is_empty(), "failure cannot hot-loop");
        program.dispatch("@0", EventKind::Click, None).unwrap();
        let (epoch, request) = take_read(&mut program);
        program
            .complete(
                epoch,
                Completion::Read(request),
                Ok(HostResponse::Route {
                    status: 503,
                    headers: BTreeMap::new(),
                    body: vec![],
                    view: None,
                }),
            )
            .unwrap();
        program.schedule_revealed();
        assert!(program.pending.is_empty());
        position(&mut program, 200);
        program.schedule_revealed();
        position(&mut program, 30);
        program.schedule_revealed();
        assert_eq!(program.pending.len(), 1, "leaving and re-entering retries");
    }

    #[test]
    fn revealed_replacement_new_sentinel_and_stale_completion() {
        let mut program = program();
        program.apply_view(View::from(document()), None).unwrap();
        program.prepare().unwrap();
        let (epoch, old) = take_read(&mut program);
        let scroll = layout::ScrollOffset { x: 0, y: 7 };
        program
            .options
            .scroll_offsets
            .insert("retained-scroll".into(), scroll);
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("sibling".into()),
                    ..Default::default()
                }),
                Some("sibling"),
            )
            .unwrap();
        assert!(
            program.revealed_current(&old),
            "unrelated sibling keeps source epoch"
        );
        program
            .complete(
                epoch,
                Completion::Read(old.clone()),
                Ok(HostResponse::View(View::from(sentinel("/page/3")))),
            )
            .unwrap();
        assert_eq!(
            program.state.find_node("@0").unwrap().route,
            sentinel("/page/3").route
        );
        assert_eq!(
            program.options.scroll_offsets.get("retained-scroll"),
            Some(&scroll)
        );
        program.prepare().unwrap();
        let (next_epoch, next) = take_read(&mut program);
        assert_ne!(old.request_id, next.request_id);
        program
            .complete(
                epoch,
                Completion::Read(old),
                Ok(HostResponse::View(View::from(sentinel("/stale")))),
            )
            .unwrap();
        assert_eq!(
            program.state.find_node("@0").unwrap().route,
            sentinel("/page/3").route
        );
        // Even an identical route at the same anonymous path has a fresh lifetime.
        program
            .apply_view(View::from(sentinel("/page/3")), Some("@0"))
            .unwrap();
        assert!(!program.revealed_current(&next));
        program
            .complete(
                next_epoch,
                Completion::Read(next),
                Ok(HostResponse::View(View::from(sentinel("/stale")))),
            )
            .unwrap();
        assert_eq!(
            program.state.find_node("@0").unwrap().route,
            sentinel("/page/3").route
        );
    }

    #[test]
    fn revealed_prepare_paint_only_and_resize_scan_current_candidate() {
        let mut program = program();
        program.apply_view(View::from(document()), None).unwrap();
        program.prepare().unwrap();
        program.pending.clear();
        program.revealed_routes.clear();
        // A stationary offscreen committed candidate must not trigger even when
        // its source document has a sentinel at the top.
        position(&mut program, 200);
        program.commit_prepared();
        program.resource_paint_only = true;
        program.fixed_image_geometry = true;
        program.geometry_dirty = false;
        program.prepare().unwrap();
        assert!(program.pending.is_empty());
        position(&mut program, 30);
        program.commit_prepared();
        program.resource_paint_only = true;
        program.prepare().unwrap();
        assert_eq!(
            program.pending.len(),
            1,
            "paint-only path scans its candidate"
        );

        let mut program = self::program();
        program.resize(Size::new(80, 24));
        program
            .apply_view(
                View::from(Container {
                    direction: LayoutDirection::Column,
                    children: vec![
                        Container {
                            height: Some(Number::Integer(640)),
                            ..Default::default()
                        },
                        sentinel("/after-resize"),
                    ],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program.prepare().unwrap();
        assert!(program.pending.is_empty());
        program.commit_prepared();
        program.resize(Size::new(80, 60));
        program.prepare().unwrap();
        assert_eq!(program.pending.len(), 1, "resize uses new viewport clip");
    }

    fn exhaust_empty_chain(program: &mut BmuxProgram) {
        let chain_id = program.revealed_routes.chain_keys["@0"];
        assert_eq!(program.revealed_routes.chains[&chain_id].admissions, 1);
        for completed in 1..=4 {
            let (epoch, request) = take_read(program);
            program
                .complete(
                    epoch,
                    Completion::Read(request),
                    Ok(HostResponse::View(View::from(sentinel("/page/2")))),
                )
                .unwrap();
            program.prepare().unwrap();
            assert_eq!(
                program.revealed_routes.chain_keys["@0"], chain_id,
                "same-path successor must inherit the original chain after completion {completed}"
            );
            assert_eq!(
                program.revealed_routes.chains[&chain_id].admissions,
                u8::try_from((completed + 1).min(4)).unwrap(),
                "stationary chain budget after completion {completed}"
            );
        }
        assert!(
            program.pending.is_empty(),
            "four stationary automatic admissions exhaust the burst"
        );
        for _ in 0..3 {
            program.prepare().unwrap();
        }
        assert!(
            program.pending.is_empty(),
            "repaint cannot restart an empty successor chain"
        );
    }

    #[test]
    fn revealed_empty_successor_chain_cap_and_scroll_resize_button_resets() {
        let mut program = program();
        program.apply_view(View::from(document()), None).unwrap();
        program.prepare().unwrap();
        exhaust_empty_chain(&mut program);
        program
            .options
            .scroll_offsets
            .insert("scroll".into(), layout::ScrollOffset { x: 0, y: 1 });
        program.schedule_revealed();
        assert!(
            program.pending.is_empty(),
            "bogus requested offset cannot reset the cap"
        );
        let root = program
            .candidate
            .as_mut()
            .map(Arc::make_mut)
            .unwrap()
            .nodes
            .first_mut()
            .unwrap();
        root.scroll_offset.y = 1;
        program.schedule_revealed();
        assert_eq!(
            program.pending.len(),
            1,
            "resolved ancestor movement resets the cap"
        );
        // Settle this request before testing the next viewport transition.
        let (epoch, request) = take_read(&mut program);
        program
            .complete(
                epoch,
                Completion::Read(request),
                Ok(HostResponse::View(View::from(sentinel("/page/2")))),
            )
            .unwrap();
        program.resize(Size::new(100, 40));
        assert!(
            program.pending.is_empty(),
            "scroll-reset request must be settled before resize"
        );
        assert_eq!(
            program.revealed_routes.entries.len(),
            0,
            "replacement must remove pending lifecycle"
        );
        program.prepare().unwrap();
        exhaust_empty_chain(&mut program);
        program.dispatch("@0", EventKind::Click, None).unwrap();
        let (epoch, request) = take_read(&mut program);
        assert!(
            program
                .revealed_routes
                .chains
                .values()
                .all(|chain| chain.admissions == 0)
        );
        program
            .complete(
                epoch,
                Completion::Read(request),
                Ok(HostResponse::View(View::from(sentinel("/page/2")))),
            )
            .unwrap();
        program.prepare().unwrap();
        assert_eq!(
            program.pending.len(),
            1,
            "explicit activation also restores automatic allowance"
        );
    }

    #[test]
    fn revealed_actions_use_exact_candidate_geometry() {
        use hyperchad_actions::{
            Action, ActionTrigger, ActionType,
            logic::{CalcValue, Value},
        };
        use hyperchad_transformer_models::ElementTarget;
        let mut program = program();
        let mut root = document();
        root.children[0].actions.push(Action {
            trigger: ActionTrigger::Event("revealed".into()),
            effect: ActionType::Parameterized {
                action: Box::new(ActionType::Custom {
                    action: "reveal-width".into(),
                }),
                value: Value::Calc(CalcValue::WidthPx {
                    target: ElementTarget::SelfTarget,
                }),
            }
            .into(),
        });
        program.apply_view(View::from(root), None).unwrap();
        program.prepare().unwrap();
        for committed in [false, true] {
            program.pending.clear();
            program.revealed_routes.clear();
            if committed {
                program.committed = Some((
                    program.state.generation(),
                    program.candidate.as_ref().unwrap().clone(),
                ));
            }
            program
                .candidate
                .as_mut()
                .map(Arc::make_mut)
                .unwrap()
                .nodes
                .iter_mut()
                .find(|node| node.key == "@0")
                .unwrap()
                .bounds
                .width = 33;
            program.schedule_revealed();
            let Some(Work::Custom(name, Some(Value::Real(width)))) = program.pending.pop_front()
            else {
                panic!("revealed action must evaluate current candidate width");
            };
            assert_eq!(name, "reveal-width");
            assert!((width - 264.0).abs() < f32::EPSILON);
            // Ensure the next iteration's committed width differs from candidate.
            program
                .candidate
                .as_mut()
                .map(Arc::make_mut)
                .unwrap()
                .nodes
                .iter_mut()
                .find(|node| node.key == "@0")
                .unwrap()
                .bounds
                .width = 20;
        }
    }

    #[test]
    fn revealed_chain_budget_survives_new_anonymous_path() {
        let mut program = program();
        program.apply_view(View::from(document()), None).unwrap();
        program.prepare().unwrap();
        let id = program.revealed_routes.chain_keys["@0"];
        let (epoch, request) = take_read(&mut program);
        program
            .complete(
                epoch,
                Completion::Read(request),
                Ok(HostResponse::View(View::from(Container {
                    children: vec![sentinel("/new-path")],
                    ..Default::default()
                }))),
            )
            .unwrap();
        program.prepare().unwrap();
        assert_eq!(program.revealed_routes.chain_keys["@0/0"], id);
        assert_eq!(program.revealed_routes.chains[&id].admissions, 2);
        assert_eq!(program.pending.len(), 1);
    }

    #[test]
    fn revealed_independent_sentinels_do_not_share_chain_budget() {
        let mut program = program();
        program
            .apply_view(
                View::from(Container {
                    direction: LayoutDirection::Column,
                    children: (0..6)
                        .map(|index| sentinel(&format!("/independent/{index}")))
                        .collect(),
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program.prepare().unwrap();
        assert_eq!(program.pending.len(), 6);
        assert_eq!(program.revealed_routes.chains.len(), 6);
        assert!(
            program
                .revealed_routes
                .chains
                .values()
                .all(|chain| chain.admissions == 1)
        );
        program.schedule_revealed();
        assert_eq!(program.pending.len(), 6, "independent routes still dedupe");
    }

    #[test]
    fn revealed_context_change_releases_pending_without_document_replacement() {
        use std::sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        };
        let context = Arc::new(AtomicU64::new(1));
        let callback = Arc::clone(&context);
        let (actions, _) = flume::bounded(8);
        let (resize, _) = flume::bounded(8);
        let mut program = BmuxProgram::new(
            BmuxConfig {
                resource_context: Some(Arc::new(move || callback.load(Ordering::SeqCst))),
                ..Default::default()
            },
            actions,
            resize,
        );
        program.apply_view(View::from(document()), None).unwrap();
        program.prepare().unwrap();
        let (epoch, old) = take_read(&mut program);
        let generation = program.state.generation();
        context.store(2, Ordering::SeqCst);
        program.revalidate_resource_context().unwrap();
        assert_eq!(program.state.generation(), generation);
        assert_eq!(program.revealed_routes.candidates.len(), 1);
        assert!(!program.revealed_current(&old));
        program.schedule_revealed();
        let (next_epoch, next) = take_read(&mut program);
        assert!(program.revealed_current(&next));
        assert_ne!(old.request_id, next.request_id);
        program
            .complete(
                epoch,
                Completion::Read(old),
                Ok(HostResponse::View(View::from(sentinel("/stale")))),
            )
            .unwrap();
        assert_eq!(
            program.state.find_node("@0").unwrap().route,
            sentinel("/page/2").route
        );
        assert!(program.revealed_current(&next));
        context.store(3, Ordering::SeqCst);
        program.revalidate_resource_context().unwrap();
        program.dispatch("@0", EventKind::Click, None).unwrap();
        let (_, retry) = take_read(&mut program);
        assert!(program.revealed_current(&retry));
        assert_ne!(next.request_id, retry.request_id);
        program
            .complete(
                next_epoch,
                Completion::Read(next),
                Ok(HostResponse::Complete),
            )
            .unwrap();
        assert!(program.revealed_current(&retry));
    }

    #[test]
    fn revealed_candidate_index_tracks_full_layout_not_every_paint() {
        let mut program = program();
        let mut root = document();
        root.children.extend((0..500).map(|_| Container::default()));
        program.apply_view(View::from(root), None).unwrap();
        program.prepare().unwrap();
        assert_eq!(program.revealed_routes.candidates.len(), 1);
        assert!(program.candidate.as_ref().unwrap().nodes.len() > 500);
        program.pending.clear();
        program
            .apply_view(View::from(Container::default()), Some("@0"))
            .unwrap();
        program.prepare().unwrap();
        assert_eq!(program.revealed_routes.candidates.len(), 0);
        assert!(
            program.pending.is_empty(),
            "removed route cannot remain a candidate"
        );
        program
            .apply_view(View::from(sentinel("/new")), Some("@0"))
            .unwrap();
        program.prepare().unwrap();
        assert_eq!(program.revealed_routes.candidates.len(), 1);
        assert_eq!(program.pending.len(), 1);
    }

    #[test]
    fn revealed_clip_bounds_and_zero_dimensions() {
        let clip = layout::CellRect::new(10, 5, 20, 10);
        assert!(near_viewport(layout::CellRect::new(10, 18, 2, 1), clip));
        assert!(!near_viewport(layout::CellRect::new(10, 22, 2, 1), clip));
        assert!(!near_viewport(layout::CellRect::new(40, 7, 2, 1), clip));
        assert!(!near_viewport(layout::CellRect::new(10, 7, 0, 1), clip));
        assert!(!near_viewport(layout::CellRect::new(10, 7, 1, 0), clip));
        assert!(!near_viewport(
            layout::CellRect::new(10, 7, 1, 1),
            layout::CellRect::default()
        ));
    }

    #[test]
    fn revealed_nested_scroll_uses_resolved_clip_and_hidden_ancestors() {
        let mut program = program();
        program.resize(Size::new(80, 40));
        let spacer = Container {
            height: Some(Number::Integer(320)),
            ..Default::default()
        };
        let scroll = Container {
            str_id: Some("scroll".into()),
            direction: LayoutDirection::Column,
            height: Some(Number::Integer(160)),
            width: Some(Number::Integer(320)),
            overflow_y: LayoutOverflow::Scroll,
            children: vec![spacer, sentinel("/nested")],
            ..Default::default()
        };
        program
            .apply_view(
                View::from(Container {
                    children: vec![scroll],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program.prepare().unwrap();
        let node = program.candidate.as_ref().unwrap().get("@0/1").unwrap();
        assert!(
            node.bounds.y < 40,
            "sentinel is inside terminal but outside nested viewport"
        );
        assert!(node.clip.height <= 10);
        assert!(program.pending.is_empty());
        program.commit_prepared();
        program.scroll_reuse = true;
        program
            .options
            .scroll_offsets
            .insert("scroll".into(), layout::ScrollOffset { x: 0, y: 6 });
        program.prepare().unwrap();
        let projection = program.candidate_projection.as_ref().unwrap();
        assert!(
            projection
                .visible_nodes()
                .iter()
                .all(|node| node.key != "@0/1")
        );
        let sentinel = projection.geometry_node("@0/1").unwrap();
        let full = layout::layout(
            program.effective.as_ref().unwrap(),
            &program.options,
            &layout::DefaultLeafMeasure,
        )
        .unwrap();
        let baseline = full.get("@0/1").unwrap();
        assert_eq!(sentinel.bounds, baseline.bounds);
        assert_eq!(sentinel.clip, baseline.clip);
        assert!(sentinel.bounds.y >= sentinel.clip.bottom());
        assert!(near_viewport(sentinel.bounds, sentinel.clip));
        assert_eq!(program.pending.len(), 1);
        program.pending.clear();
        program.revealed_routes.clear();
        program.candidate_projection = None;
        let tree = Arc::make_mut(program.candidate.as_mut().unwrap());
        tree.nodes
            .iter_mut()
            .find(|node| node.key == "scroll")
            .unwrap()
            .visible = false;
        program.schedule_revealed();
        assert!(program.pending.is_empty());
    }
}
