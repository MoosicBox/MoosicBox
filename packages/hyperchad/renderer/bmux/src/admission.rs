//! Allocation-bounded admission before cloning or interpreting untrusted views.
//!
//! String bytes and structural entries share the byte budget (one byte per
//! visited payload entry). Container count is separate. Nested payloads use the
//! configured depth limit, capped at 256 to protect downstream recursive clones.

use hyperchad_actions::{ActionEffect, ActionTrigger, ActionType, StyleAction, dsl, logic};
use hyperchad_renderer::{ResponseNavigation, View};
use hyperchad_transformer::{
    Calculation, Container, Element, Input, Number, OverrideCondition, OverrideItem,
};
use hyperchad_transformer_models::{ElementTarget, Route, Selector, Target};

use crate::renderer::{BmuxConfig, Error};

/// Validate a response, including fragments, selectors and response metadata.
///
/// # Errors
/// * Rejects excessive container count, nesting, payload work or aggregate bytes.
/// * Error messages never include document contents or secrets.
pub fn validate_view(view: &View, config: &BmuxConfig) -> Result<(), Error> {
    let mut budget = Budget::new(config);
    if let Some(root) = &view.primary {
        budget.push(Item::Node(root), 0)?;
    }
    budget.entries(view.fragments.len())?;
    for fragment in &view.fragments {
        budget.selector(&fragment.selector)?;
        budget.push(Item::Node(&fragment.container), 0)?;
    }
    budget.entries(view.delete_selectors.len())?;
    for selector in &view.delete_selectors {
        budget.selector(selector)?;
    }
    budget.entries(view.response.cookies.len())?;
    for cookie in &view.response.cookies {
        budget.text(&cookie.name)?;
        budget.text(&cookie.value)?;
        budget.text(&cookie.path)?;
    }
    if let Some(ResponseNavigation::Internal(url) | ResponseNavigation::External(url)) =
        &view.response.navigation
    {
        budget.text(url)?;
    }
    budget.run()
}

/// Validate the complete resulting canonical tree, not just an incoming delta.
///
/// # Errors
/// * Rejects excessive container count, nesting, payload work or aggregate bytes.
/// * Validation borrows the tree and never clones or formats its payloads.
pub fn validate_tree(root: &Container, config: &BmuxConfig) -> Result<(), Error> {
    let mut budget = Budget::new(config);
    budget.push(Item::Node(root), 0)?;
    budget.run()
}

fn limit(message: &'static str) -> Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message).into()
}

enum Item<'a> {
    Node(&'a Container),
    Number(&'a Number),
    Calculation(&'a Calculation),
    Effect(&'a ActionEffect),
    Action(&'a ActionType),
    Expression(&'a dsl::Expression),
    Statement(&'a dsl::Statement),
    Pattern(&'a dsl::Pattern),
    Value(&'a logic::Value),
    Arithmetic(&'a logic::Arithmetic),
    Json(&'a serde_json::Value),
    Override(&'a OverrideItem),
}

struct Budget<'a> {
    config: &'a BmuxConfig,
    bytes: usize,
    nodes: usize,
    scheduled: usize,
    stack: Vec<(Item<'a>, usize)>,
}

impl<'a> Budget<'a> {
    const fn new(config: &'a BmuxConfig) -> Self {
        Self {
            config,
            bytes: 0,
            nodes: 0,
            scheduled: 0,
            stack: Vec::new(),
        }
    }

    fn entries(&mut self, bytes: usize) -> Result<(), Error> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| limit("bmux admission byte budget exceeded"))?;
        if self.bytes > self.config.max_text_bytes {
            return Err(limit("bmux admission byte budget exceeded"));
        }
        Ok(())
    }

    fn text(&mut self, text: &str) -> Result<(), Error> {
        self.entries(text.len())
    }
    fn optional(&mut self, text: Option<&String>) -> Result<(), Error> {
        if let Some(text) = text {
            self.text(text)?;
        }
        Ok(())
    }
    fn strings(&mut self, strings: &[String]) -> Result<(), Error> {
        self.entries(strings.len())?;
        for text in strings {
            self.text(text)?;
        }
        Ok(())
    }
    fn push(&mut self, item: Item<'a>, depth: usize) -> Result<(), Error> {
        if depth > self.config.max_depth.min(256) {
            return Err(limit("bmux admission nesting limit exceeded"));
        }
        // Reserve budget before growing the work list, never after collecting it.
        self.entries(1)?;
        self.scheduled = self
            .scheduled
            .checked_add(1)
            .ok_or_else(|| limit("bmux admission payload work limit exceeded"))?;
        if self.scheduled > self.config.max_nodes.saturating_mul(64) {
            return Err(limit("bmux admission payload work limit exceeded"));
        }
        if matches!(item, Item::Node(_)) {
            self.nodes = self
                .nodes
                .checked_add(1)
                .ok_or_else(|| limit("bmux admission node limit exceeded"))?;
            if self.nodes > self.config.max_nodes {
                return Err(limit("bmux admission node limit exceeded"));
            }
        }
        self.stack.push((item, depth));
        Ok(())
    }
    fn selector(&mut self, selector: &Selector) -> Result<(), Error> {
        match selector {
            Selector::Id(x) | Selector::Class(x) | Selector::ChildClass(x) => self.text(x),
            Selector::SelfTarget => Ok(()),
        }
    }
    fn target(&mut self, target: &ElementTarget) -> Result<(), Error> {
        match target {
            ElementTarget::ById(Target::Literal(x) | Target::Ref(x))
            | ElementTarget::Selector(Target::Literal(x) | Target::Ref(x))
            | ElementTarget::Class(Target::Literal(x) | Target::Ref(x))
            | ElementTarget::ChildClass(Target::Literal(x) | Target::Ref(x)) => self.text(x),
            ElementTarget::Id(_) | ElementTarget::SelfTarget | ElementTarget::LastChild => Ok(()),
        }
    }
    fn number(&mut self, number: Option<&'a Number>, depth: usize) -> Result<(), Error> {
        if let Some(number) = number {
            self.push(Item::Number(number), depth)?;
        }
        Ok(())
    }
    fn effects(&mut self, effects: &'a [ActionEffect], depth: usize) -> Result<(), Error> {
        self.entries(effects.len())?;
        for effect in effects {
            self.push(Item::Effect(effect), depth)?;
        }
        Ok(())
    }
    fn expressions(
        &mut self,
        expressions: &'a [dsl::Expression],
        depth: usize,
    ) -> Result<(), Error> {
        self.entries(expressions.len())?;
        for expression in expressions {
            self.push(Item::Expression(expression), depth)?;
        }
        Ok(())
    }
    fn block(&mut self, block: &'a dsl::Block, depth: usize) -> Result<(), Error> {
        self.entries(block.statements.len())?;
        for statement in &block.statements {
            self.push(Item::Statement(statement), depth)?;
        }
        Ok(())
    }
    fn arms(&mut self, arms: &'a [dsl::MatchArm], depth: usize) -> Result<(), Error> {
        self.entries(arms.len())?;
        for arm in arms {
            self.push(Item::Pattern(&arm.pattern), depth)?;
            self.push(Item::Expression(&arm.body), depth)?;
        }
        Ok(())
    }
    fn literal(&mut self, literal: &dsl::Literal) -> Result<(), Error> {
        if let dsl::Literal::String(text) = literal {
            self.text(text)?;
        }
        Ok(())
    }
    fn condition(&mut self, condition: &'a logic::Condition, depth: usize) -> Result<(), Error> {
        if let logic::Condition::Eq(a, b) = condition {
            self.push(Item::Value(a), depth)?;
            self.push(Item::Value(b), depth)?;
        }
        Ok(())
    }
    fn run(&mut self) -> Result<(), Error> {
        while let Some((item, depth)) = self.stack.pop() {
            let next = depth + 1;
            match item {
                Item::Node(node) => self.node(node, depth)?,
                Item::Number(Number::Calc(calc)) => self.push(Item::Calculation(calc), next)?,
                Item::Number(_) => {}
                Item::Calculation(calc) => match calc {
                    Calculation::Number(number) => self.push(Item::Number(number), next)?,
                    Calculation::Grouping(calc) => self.push(Item::Calculation(calc), next)?,
                    Calculation::Add(a, b)
                    | Calculation::Subtract(a, b)
                    | Calculation::Multiply(a, b)
                    | Calculation::Divide(a, b)
                    | Calculation::Min(a, b)
                    | Calculation::Max(a, b) => {
                        self.push(Item::Calculation(a), next)?;
                        self.push(Item::Calculation(b), next)?;
                    }
                },
                Item::Effect(effect) => self.push(Item::Action(&effect.action), next)?,
                Item::Action(action) => self.action(action, next)?,
                Item::Expression(expression) => self.expression(expression, next)?,
                Item::Statement(statement) => self.statement(statement, next)?,
                Item::Pattern(pattern) => match pattern {
                    dsl::Pattern::Literal(x) => self.literal(x)?,
                    dsl::Pattern::Variable(x) => self.text(x)?,
                    dsl::Pattern::Wildcard => {}
                    dsl::Pattern::Variant {
                        enum_name,
                        variant,
                        fields,
                    } => {
                        self.text(enum_name)?;
                        self.text(variant)?;
                        self.entries(fields.len())?;
                        for field in fields {
                            self.push(Item::Pattern(field), next)?;
                        }
                    }
                },
                Item::Value(value) => self.value(value, next)?,
                Item::Arithmetic(arithmetic) => match arithmetic {
                    logic::Arithmetic::Grouping(x) => self.push(Item::Arithmetic(x), next)?,
                    logic::Arithmetic::Plus(a, b)
                    | logic::Arithmetic::Minus(a, b)
                    | logic::Arithmetic::Multiply(a, b)
                    | logic::Arithmetic::Divide(a, b)
                    | logic::Arithmetic::Min(a, b)
                    | logic::Arithmetic::Max(a, b) => {
                        self.push(Item::Value(a), next)?;
                        self.push(Item::Value(b), next)?;
                    }
                },
                Item::Json(json) => match json {
                    serde_json::Value::String(x) => self.text(x)?,
                    serde_json::Value::Array(values) => {
                        self.entries(values.len())?;
                        for value in values {
                            self.push(Item::Json(value), next)?;
                        }
                    }
                    serde_json::Value::Object(values) => {
                        self.entries(values.len())?;
                        for (key, value) in values {
                            self.text(key)?;
                            self.push(Item::Json(value), next)?;
                        }
                    }
                    _ => {}
                },
                Item::Override(item) => self.style_override(item, next)?,
            }
        }
        Ok(())
    }

    fn action(&mut self, action: &'a ActionType, depth: usize) -> Result<(), Error> {
        match action {
            ActionType::NoOp => {}
            ActionType::Input(hyperchad_actions::InputActionType::Select { target }) => {
                self.target(target)?;
            }
            ActionType::Let { name, value } => {
                self.text(name)?;
                self.push(Item::Expression(value), depth)?;
            }
            ActionType::Style { target, action } => {
                self.target(target)?;
                if let StyleAction::SetBackground(Some(x)) = action {
                    self.text(x)?;
                }
            }
            ActionType::Navigate { url } => self.text(url)?,
            ActionType::Log { message, .. } => self.text(message)?,
            ActionType::Custom { action } => self.text(action)?,
            ActionType::Event { name, action } => {
                self.text(name)?;
                self.push(Item::Action(action), depth)?;
            }
            ActionType::Multi(actions) => {
                self.entries(actions.len())?;
                for action in actions {
                    self.push(Item::Action(action), depth)?;
                }
            }
            ActionType::MultiEffect(effects) => self.effects(effects, depth)?,
            ActionType::Parameterized { action, value } => {
                self.push(Item::Action(action), depth)?;
                self.push(Item::Value(value), depth)?;
            }
            ActionType::Logic(logic) => {
                self.condition(&logic.condition, depth)?;
                self.effects(&logic.actions, depth)?;
                self.effects(&logic.else_actions, depth)?;
            }
        }
        Ok(())
    }
    fn expression(&mut self, expression: &'a dsl::Expression, depth: usize) -> Result<(), Error> {
        use dsl::Expression as E;
        match expression {
            E::Literal(x) => self.literal(x)?,
            E::Variable(x) | E::RawRust(x) => self.text(x)?,
            E::ElementRef(x) | E::ElementByIdRef(x) | E::Grouping(x) | E::Unary { expr: x, .. } => {
                self.push(Item::Expression(x), depth)?;
            }
            E::Call { function, args } => {
                self.text(function)?;
                self.expressions(args, depth)?;
            }
            E::MethodCall {
                receiver,
                method,
                args,
            } => {
                self.push(Item::Expression(receiver), depth)?;
                self.text(method)?;
                self.expressions(args, depth)?;
            }
            E::Field { object, field } => {
                self.push(Item::Expression(object), depth)?;
                self.text(field)?;
            }
            E::Binary { left, right, .. } => {
                self.push(Item::Expression(left), depth)?;
                self.push(Item::Expression(right), depth)?;
            }
            E::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.push(Item::Expression(condition), depth)?;
                self.push(Item::Expression(then_branch), depth)?;
                if let Some(x) = else_branch {
                    self.push(Item::Expression(x), depth)?;
                }
            }
            E::Match { expr, arms } => {
                self.push(Item::Expression(expr), depth)?;
                self.arms(arms, depth)?;
            }
            E::Block(block) => self.block(block, depth)?,
            E::Array(values) | E::Tuple(values) => self.expressions(values, depth)?,
            E::Range { start, end, .. } => {
                for x in [start, end].into_iter().flatten() {
                    self.push(Item::Expression(x), depth)?;
                }
            }
            E::Closure { params, body } => {
                self.strings(params)?;
                self.push(Item::Expression(body), depth)?;
            }
        }
        Ok(())
    }
    fn statement(&mut self, statement: &'a dsl::Statement, depth: usize) -> Result<(), Error> {
        use dsl::Statement as S;
        match statement {
            S::Expression(x) => self.push(Item::Expression(x), depth)?,
            S::Let { name, value } => {
                self.text(name)?;
                self.push(Item::Expression(value), depth)?;
            }
            S::If {
                condition,
                then_block,
                else_block,
            } => {
                self.push(Item::Expression(condition), depth)?;
                self.block(then_block, depth)?;
                if let Some(block) = else_block {
                    self.block(block, depth)?;
                }
            }
            S::Match { expr, arms } => {
                self.push(Item::Expression(expr), depth)?;
                self.arms(arms, depth)?;
            }
            S::For {
                pattern,
                iter,
                body,
            } => {
                self.text(pattern)?;
                self.push(Item::Expression(iter), depth)?;
                self.block(body, depth)?;
            }
            S::While { condition, body } => {
                self.push(Item::Expression(condition), depth)?;
                self.block(body, depth)?;
            }
            S::Block(block) => self.block(block, depth)?,
        }
        Ok(())
    }
    fn value(&mut self, value: &'a logic::Value, depth: usize) -> Result<(), Error> {
        use logic::{CalcValue, Value};
        match value {
            Value::String(x) => self.text(x)?,
            Value::Arithmetic(x) => self.push(Item::Arithmetic(x), depth)?,
            Value::Calc(x) => match x {
                CalcValue::Visibility { target }
                | CalcValue::Display { target }
                | CalcValue::Id { target }
                | CalcValue::WidthPx { target }
                | CalcValue::HeightPx { target }
                | CalcValue::PositionX { target }
                | CalcValue::PositionY { target } => self.target(target)?,
                CalcValue::DataAttrValue { attr, target } => {
                    self.text(attr)?;
                    self.target(target)?;
                }
                CalcValue::MouseX { target } | CalcValue::MouseY { target } => {
                    if let Some(target) = target {
                        self.target(target)?;
                    }
                }
                CalcValue::EventValue | CalcValue::Key { .. } => {}
            },
            Value::Real(_)
            | Value::Visibility(_)
            | Value::Display(_)
            | Value::LayoutDirection(_)
            | Value::Key(_) => {}
        }
        Ok(())
    }

    fn element(&mut self, element: &'a Element, depth: usize) -> Result<(), Error> {
        match element {
            Element::Raw { value } | Element::Text { value } => self.text(value)?,
            Element::Form { action, method, .. } => {
                self.optional(action.as_ref())?;
                self.optional(method.as_ref())?;
            }
            Element::Input { input, name, .. } => {
                self.optional(name.as_ref())?;
                match input {
                    Input::Text { value, placeholder } | Input::Password { value, placeholder } => {
                        self.optional(value.as_ref())?;
                        self.optional(placeholder.as_ref())?;
                    }
                    Input::Hidden { value } => self.optional(value.as_ref())?,
                    Input::Checkbox { .. } | Input::File => {}
                }
            }
            Element::Button { r#type } => self.optional(r#type.as_ref())?,
            Element::Image {
                source,
                alt,
                source_set,
                sizes,
                ..
            } => {
                self.optional(source.as_ref())?;
                self.optional(alt.as_ref())?;
                self.optional(source_set.as_ref())?;
                self.number(sizes.as_ref(), depth)?;
            }
            Element::Anchor { href, target } => {
                self.optional(href.as_ref())?;
                if let Some(hyperchad_transformer_models::LinkTarget::Custom(name)) = target {
                    self.text(name)?;
                }
            }
            Element::Textarea {
                value,
                placeholder,
                name,
                rows,
                cols,
            } => {
                self.text(value)?;
                self.optional(placeholder.as_ref())?;
                self.optional(name.as_ref())?;
                self.number(rows.as_ref(), depth)?;
                self.number(cols.as_ref(), depth)?;
            }
            Element::Select { name, selected, .. } => {
                self.optional(name.as_ref())?;
                self.optional(selected.as_ref())?;
            }
            Element::Option { value, .. } => self.optional(value.as_ref())?,
            Element::TH { rows, columns } | Element::TD { rows, columns } => {
                self.number(rows.as_ref(), depth)?;
                self.number(columns.as_ref(), depth)?;
            }
            Element::Div
            | Element::Aside
            | Element::Main
            | Element::Header
            | Element::Footer
            | Element::Section
            | Element::Span
            | Element::Heading { .. }
            | Element::UnorderedList
            | Element::OrderedList
            | Element::ListItem
            | Element::Table
            | Element::THead
            | Element::TBody
            | Element::TR
            | Element::Canvas
            | Element::Details { .. }
            | Element::Summary => {}
        }
        Ok(())
    }

    fn decoration(
        &mut self,
        decoration: &'a hyperchad_transformer::TextDecoration,
        depth: usize,
    ) -> Result<(), Error> {
        self.entries(decoration.line.len())?;
        self.number(decoration.thickness.as_ref(), depth)
    }

    fn style_override(&mut self, item: &'a OverrideItem, depth: usize) -> Result<(), Error> {
        use OverrideItem as O;
        match item {
            O::StrId(x) => self.text(x)?,
            O::Classes(x) | O::FontFamily(x) => self.strings(x)?,
            O::TextDecoration(x) => self.decoration(x, depth)?,
            O::Flex(x) => {
                self.push(Item::Number(&x.grow), depth)?;
                self.push(Item::Number(&x.shrink), depth)?;
                self.push(Item::Number(&x.basis), depth)?;
            }
            O::GridCellSize(x)
            | O::Width(x)
            | O::MinWidth(x)
            | O::MaxWidth(x)
            | O::Height(x)
            | O::MinHeight(x)
            | O::MaxHeight(x)
            | O::ColumnGap(x)
            | O::RowGap(x)
            | O::Opacity(x)
            | O::Left(x)
            | O::Right(x)
            | O::Top(x)
            | O::Bottom(x)
            | O::TranslateX(x)
            | O::TranslateY(x)
            | O::BorderTopLeftRadius(x)
            | O::BorderTopRightRadius(x)
            | O::BorderBottomLeftRadius(x)
            | O::BorderBottomRightRadius(x)
            | O::MarginLeft(x)
            | O::MarginRight(x)
            | O::MarginTop(x)
            | O::MarginBottom(x)
            | O::PaddingLeft(x)
            | O::PaddingRight(x)
            | O::PaddingTop(x)
            | O::PaddingBottom(x)
            | O::FontSize(x)
            | O::BorderTop((_, x))
            | O::BorderRight((_, x))
            | O::BorderBottom((_, x))
            | O::BorderLeft((_, x)) => self.push(Item::Number(x), depth)?,
            O::Direction(_)
            | O::OverflowX(_)
            | O::OverflowY(_)
            | O::JustifyContent(_)
            | O::AlignItems(_)
            | O::TextAlign(_)
            | O::WhiteSpace(_)
            | O::FontWeight(_)
            | O::Cursor(_)
            | O::UserSelect(_)
            | O::OverflowWrap(_)
            | O::TextOverflow(_)
            | O::Position(_)
            | O::Background(_)
            | O::Color(_)
            | O::Hidden(_)
            | O::Visibility(_) => {}
        }
        Ok(())
    }

    fn node_style(&mut self, node: &'a Container, next: usize) -> Result<(), Error> {
        if let Some(families) = &node.font_family {
            self.strings(families)?;
        }
        if let Some(decoration) = &node.text_decoration {
            self.decoration(decoration, next)?;
        }
        if let Some(flex) = &node.flex {
            for number in [&flex.grow, &flex.shrink, &flex.basis] {
                self.push(Item::Number(number), next)?;
            }
        }
        for number in [
            node.grid_cell_size.as_ref(),
            node.width.as_ref(),
            node.min_width.as_ref(),
            node.max_width.as_ref(),
            node.height.as_ref(),
            node.min_height.as_ref(),
            node.max_height.as_ref(),
            node.column_gap.as_ref(),
            node.row_gap.as_ref(),
            node.opacity.as_ref(),
            node.left.as_ref(),
            node.right.as_ref(),
            node.top.as_ref(),
            node.bottom.as_ref(),
            node.translate_x.as_ref(),
            node.translate_y.as_ref(),
            node.border_top.as_ref().map(|(_, n)| n),
            node.border_right.as_ref().map(|(_, n)| n),
            node.border_bottom.as_ref().map(|(_, n)| n),
            node.border_left.as_ref().map(|(_, n)| n),
            node.border_top_left_radius.as_ref(),
            node.border_top_right_radius.as_ref(),
            node.border_bottom_left_radius.as_ref(),
            node.border_bottom_right_radius.as_ref(),
            node.margin_left.as_ref(),
            node.margin_right.as_ref(),
            node.margin_top.as_ref(),
            node.margin_bottom.as_ref(),
            node.padding_left.as_ref(),
            node.padding_right.as_ref(),
            node.padding_top.as_ref(),
            node.padding_bottom.as_ref(),
            node.font_size.as_ref(),
        ] {
            self.number(number, next)?;
        }
        Ok(())
    }

    fn node(&mut self, node: &'a Container, depth: usize) -> Result<(), Error> {
        let next = depth + 1;
        self.optional(node.str_id.as_ref())?;
        self.strings(&node.classes)?;
        self.entries(node.data.len())?;
        for (key, value) in &node.data {
            self.text(key)?;
            self.text(value)?;
        }
        self.element(&node.element, next)?;
        self.node_style(node, next)?;
        if let Some(state) = &node.state {
            self.push(Item::Json(state), next)?;
        }
        if let Some(route) = &node.route {
            let (route, trigger, target) = match route {
                Route::Get {
                    route,
                    trigger,
                    target,
                    ..
                }
                | Route::Post {
                    route,
                    trigger,
                    target,
                    ..
                }
                | Route::Put {
                    route,
                    trigger,
                    target,
                    ..
                }
                | Route::Delete {
                    route,
                    trigger,
                    target,
                    ..
                }
                | Route::Patch {
                    route,
                    trigger,
                    target,
                    ..
                } => (route, trigger, target),
            };
            self.text(route)?;
            self.optional(trigger.as_ref())?;
            self.selector(target)?;
        }
        self.entries(node.actions.len())?;
        for action in &node.actions {
            if let ActionTrigger::Event(name) = &action.trigger {
                self.text(name)?;
            }
            self.push(Item::Effect(&action.effect), next)?;
        }
        self.entries(node.overrides.len())?;
        for config in &node.overrides {
            let OverrideCondition::ResponsiveTarget { name } = &config.condition;
            self.text(name)?;
            self.entries(config.overrides.len())?;
            for item in &config.overrides {
                self.push(Item::Override(item), next)?;
            }
            if let Some(item) = &config.default {
                self.push(Item::Override(item), next)?;
            }
        }
        // Reserve every child before adding a reference to the explicit stack.
        for child in &node.children {
            self.push(Item::Node(child), next)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
