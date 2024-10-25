#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
pub struct Viewport {
    pub parent: Option<Box<Viewport>>,
    pub pos: Pos,
    pub viewport: Pos,
}

impl Viewport {
    fn is_visible(&self) -> (bool, f32) {
        if let Some((visible, dist)) = self.parent.as_ref().map(|x| x.is_visible()) {
            if visible {
                let pos = self.pos;
                let vp = self.viewport;
                super::is_visible(vp.x, vp.y, vp.w, vp.h, pos.x, pos.y, pos.w, pos.h)
            } else {
                (false, dist)
            }
        } else {
            (true, 0.0)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct ViewportListener {
    pub viewport: Option<Viewport>,
    visible: bool,
    prev_visible: Option<bool>,
    initialized: bool,
    dist: f32,
    prev_dist: Option<f32>,
    pub pos: Pos,
}

impl ViewportListener {
    #[must_use]
    pub const fn new(viewport: Option<Viewport>, x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            viewport,
            visible: false,
            prev_visible: None,
            initialized: false,
            dist: 0.0,
            prev_dist: None,
            pos: Pos { x, y, w, h },
        }
    }

    fn is_visible(&self) -> (bool, f32) {
        if let Some(((visible, dist), vp, pos)) = self
            .viewport
            .as_ref()
            .map(|x| (x.is_visible(), x.viewport, x.pos))
        {
            if visible {
                super::is_visible(
                    vp.x + pos.x,
                    vp.y + pos.y,
                    vp.w,
                    vp.h,
                    self.pos.x,
                    self.pos.y,
                    self.pos.w,
                    self.pos.h,
                )
            } else {
                (false, dist)
            }
        } else {
            (true, 0.0)
        }
    }

    pub fn check(&mut self) -> ((bool, Option<bool>), (f32, Option<f32>)) {
        let (visible, dist) = self.is_visible();
        log::trace!("check: pos={:?} visible={visible} dist={dist}", self.pos);

        if self.initialized {
            let prev_visible = self.visible;
            let prev_dist = self.dist;
            self.prev_visible = if prev_visible == visible {
                None
            } else {
                self.visible = visible;
                Some(prev_visible)
            };
            self.prev_dist = if (prev_dist - dist) < 0.01 {
                None
            } else {
                self.dist = dist;
                Some(prev_dist)
            };

            ((visible, self.prev_visible), (dist, self.prev_dist))
        } else {
            self.initialized = true;
            self.visible = visible;
            self.dist = dist;
            ((visible, None), (dist, None))
        }
    }
}
