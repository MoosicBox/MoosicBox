//! Admission checks before retaining/cloning recursive action data.
use super::{ActionError, ActionLimits};
use hyperchad_actions::{
    ActionType, ElementTarget, Target,
    dsl::{Block, Expression, Literal, Pattern, Statement},
    logic::{Arithmetic, CalcValue, Condition, Value},
};

pub(super) fn action(action: &ActionType, limits: &ActionLimits) -> Result<(), ActionError> {
    Check {
        limits,
        steps: 0,
        bytes: 0,
    }
    .action(action, 0)
}
struct Check<'a> {
    limits: &'a ActionLimits,
    steps: usize,
    bytes: usize,
}
impl Check<'_> {
    const fn step(&mut self, depth: usize, length: usize) -> Result<(), ActionError> {
        self.steps = self.steps.saturating_add(1);
        if depth > self.limits.max_depth
            || self.steps > self.limits.max_steps
            || length > self.limits.max_entries
        {
            return Err(ActionError::Limit("AST depth/steps/entries"));
        }
        Ok(())
    }
    const fn text(&mut self, text: &str) -> Result<(), ActionError> {
        self.bytes = self.bytes.saturating_add(text.len());
        if self.bytes > self.limits.max_value_bytes {
            return Err(ActionError::Limit("AST bytes"));
        }
        Ok(())
    }
    fn target(&mut self, target: &ElementTarget) -> Result<(), ActionError> {
        match target {
            ElementTarget::ById(t)
            | ElementTarget::Class(t)
            | ElementTarget::ChildClass(t)
            | ElementTarget::Selector(t) => match t {
                Target::Literal(v) | Target::Ref(v) => self.text(v),
            },
            ElementTarget::Id(_) | ElementTarget::SelfTarget | ElementTarget::LastChild => Ok(()),
        }
    }
    fn action(&mut self, action: &ActionType, d: usize) -> Result<(), ActionError> {
        self.step(d, 0)?;
        match action {
            ActionType::NoOp => {}
            ActionType::Let { name, value } => {
                self.text(name)?;
                self.expr(value, d + 1)?;
            }
            ActionType::Style { target, action } => {
                self.target(target)?;
                if let hyperchad_actions::StyleAction::SetBackground(Some(v)) = action {
                    self.text(v)?;
                }
            }
            ActionType::Input(hyperchad_actions::InputActionType::Select { target }) => {
                self.target(target)?;
            }
            ActionType::Navigate { url } => self.text(url)?,
            ActionType::Log { message, .. } => self.text(message)?,
            ActionType::Custom { action } => self.text(action)?,
            ActionType::Event { name, action } => {
                self.text(name)?;
                self.action(action, d + 1)?;
            }
            ActionType::Parameterized { action, value } => {
                self.value(value, d + 1)?;
                self.action(action, d + 1)?;
            }
            ActionType::Multi(values) => {
                self.step(d, values.len())?;
                for v in values {
                    self.action(v, d + 1)?;
                }
            }
            ActionType::MultiEffect(values) => {
                self.step(d, values.len())?;
                for v in values {
                    self.action(&v.action, d + 1)?;
                }
            }
            ActionType::Logic(value) => {
                if let Condition::Eq(a, b) = &value.condition {
                    self.value(a, d + 1)?;
                    self.value(b, d + 1)?;
                }
                self.step(
                    d,
                    value.actions.len().saturating_add(value.else_actions.len()),
                )?;
                for v in value.actions.iter().chain(&value.else_actions) {
                    self.action(&v.action, d + 1)?;
                }
            }
        }
        Ok(())
    }
    fn value(&mut self, value: &Value, d: usize) -> Result<(), ActionError> {
        self.step(d, 0)?;
        match value {
            Value::String(v) => self.text(v)?,
            Value::Arithmetic(v) => self.arithmetic(v, d + 1)?,
            Value::Calc(v) => match v {
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
                    if let Some(t) = target {
                        self.target(t)?;
                    }
                }
                CalcValue::Key { .. } | CalcValue::EventValue => {}
            },
            Value::Real(_)
            | Value::Visibility(_)
            | Value::Display(_)
            | Value::Key(_)
            | Value::LayoutDirection(_) => {}
        }
        Ok(())
    }
    fn arithmetic(&mut self, value: &Arithmetic, d: usize) -> Result<(), ActionError> {
        self.step(d, 0)?;
        match value {
            Arithmetic::Grouping(v) => self.arithmetic(v, d + 1)?,
            Arithmetic::Plus(a, b)
            | Arithmetic::Minus(a, b)
            | Arithmetic::Multiply(a, b)
            | Arithmetic::Divide(a, b)
            | Arithmetic::Min(a, b)
            | Arithmetic::Max(a, b) => {
                self.value(a, d + 1)?;
                self.value(b, d + 1)?;
            }
        }
        Ok(())
    }
    fn literal(&mut self, value: &Literal) -> Result<(), ActionError> {
        if let Literal::String(v) = value {
            self.text(v)?;
        }
        Ok(())
    }
    fn pattern(&mut self, value: &Pattern, d: usize) -> Result<(), ActionError> {
        self.step(d, 0)?;
        match value {
            Pattern::Literal(v) => self.literal(v)?,
            Pattern::Variable(v) => self.text(v)?,
            Pattern::Wildcard => {}
            Pattern::Variant {
                enum_name,
                variant,
                fields,
            } => {
                self.text(enum_name)?;
                self.text(variant)?;
                self.step(d, fields.len())?;
                for f in fields {
                    self.pattern(f, d + 1)?;
                }
            }
        }
        Ok(())
    }
    fn exprs(&mut self, values: &[Expression], d: usize) -> Result<(), ActionError> {
        self.step(d, values.len())?;
        for v in values {
            self.expr(v, d + 1)?;
        }
        Ok(())
    }
    fn expr(&mut self, value: &Expression, d: usize) -> Result<(), ActionError> {
        self.step(d, 0)?;
        match value {
            Expression::Literal(v) => self.literal(v)?,
            Expression::Variable(v) | Expression::RawRust(v) => self.text(v)?,
            Expression::ElementRef(v)
            | Expression::ElementByIdRef(v)
            | Expression::Grouping(v)
            | Expression::Unary { expr: v, .. } => self.expr(v, d + 1)?,
            Expression::Call { function, args } => {
                self.text(function)?;
                self.exprs(args, d + 1)?;
            }
            Expression::MethodCall {
                receiver,
                method,
                args,
            } => {
                self.expr(receiver, d + 1)?;
                self.text(method)?;
                self.exprs(args, d + 1)?;
            }
            Expression::Field { object, field } => {
                self.expr(object, d + 1)?;
                self.text(field)?;
            }
            Expression::Binary { left, right, .. } => {
                self.expr(left, d + 1)?;
                self.expr(right, d + 1)?;
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(condition, d + 1)?;
                self.expr(then_branch, d + 1)?;
                if let Some(v) = else_branch {
                    self.expr(v, d + 1)?;
                }
            }
            Expression::Match { expr, arms } => {
                self.expr(expr, d + 1)?;
                self.step(d, arms.len())?;
                for arm in arms {
                    self.pattern(&arm.pattern, d + 1)?;
                    self.expr(&arm.body, d + 1)?;
                }
            }
            Expression::Block(v) => self.block(v, d + 1)?,
            Expression::Array(v) | Expression::Tuple(v) => self.exprs(v, d + 1)?,
            Expression::Range { start, end, .. } => {
                for v in start.iter().chain(end) {
                    self.expr(v, d + 1)?;
                }
            }
            Expression::Closure { params, body } => {
                self.step(d, params.len())?;
                for p in params {
                    self.text(p)?;
                }
                self.expr(body, d + 1)?;
            }
        }
        Ok(())
    }
    fn block(&mut self, block: &Block, d: usize) -> Result<(), ActionError> {
        self.step(d, block.statements.len())?;
        for statement in &block.statements {
            match statement {
                Statement::Expression(v) => self.expr(v, d + 1)?,
                Statement::Let { name, value } => {
                    self.text(name)?;
                    self.expr(value, d + 1)?;
                }
                Statement::Block(v) => self.block(v, d + 1)?,
                Statement::If {
                    condition,
                    then_block,
                    else_block,
                } => {
                    self.expr(condition, d + 1)?;
                    self.block(then_block, d + 1)?;
                    if let Some(v) = else_block {
                        self.block(v, d + 1)?;
                    }
                }
                Statement::For {
                    pattern,
                    iter,
                    body,
                } => {
                    self.text(pattern)?;
                    self.expr(iter, d + 1)?;
                    self.block(body, d + 1)?;
                }
                Statement::While { condition, body } => {
                    self.expr(condition, d + 1)?;
                    self.block(body, d + 1)?;
                }
                Statement::Match { expr, arms } => {
                    self.expr(expr, d + 1)?;
                    self.step(d, arms.len())?;
                    for arm in arms {
                        self.pattern(&arm.pattern, d + 1)?;
                        self.expr(&arm.body, d + 1)?;
                    }
                }
            }
        }
        Ok(())
    }
}
