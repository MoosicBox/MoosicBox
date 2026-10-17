//! Bounded expression evaluation. Backend-specific Rust/closures are rejected explicitly.
use super::{ActionError, ActionLimits, Geometry, Owner, Style, Work, selectors};
use hyperchad_actions::{
    ActionType, ElementTarget, Target,
    dsl::{BinaryOp, Block, Expression, Literal, Pattern, Statement, UnaryOp},
    logic::{Arithmetic, CalcValue, Value},
};
use hyperchad_transformer_models::Visibility;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Datum {
    Value(Value),
    Bool(bool),
    Integer(i64),
    Array(Vec<Self>),
    Element(Vec<usize>),
    Unit,
}
impl Datum {
    fn bytes(&self) -> usize {
        match self {
            Self::Value(Value::String(v)) => v.len(),
            Self::Array(v) => v.iter().fold(0_usize, |n, v| {
                n.saturating_add(v.bytes())
                    .saturating_add(std::mem::size_of::<Self>())
            }),
            Self::Element(path) => path.len().saturating_mul(std::mem::size_of::<usize>()),
            _ => std::mem::size_of::<Self>(),
        }
    }
    pub(super) fn string(&self) -> Result<String, ActionError> {
        match self {
            Self::Value(Value::String(value)) => Ok(value.clone()),
            Self::Integer(value) => Ok(value.to_string()),
            Self::Bool(value) => Ok(value.to_string()),
            Self::Value(Value::Real(value)) => Ok(value.to_string()),
            _ => Err(ActionError::Invalid("expected scalar string".into())),
        }
    }
    fn bool(&self) -> Result<bool, ActionError> {
        match self {
            Self::Bool(v) | Self::Value(Value::Display(v)) => Ok(*v),
            _ => Err(ActionError::Invalid("expected boolean".into())),
        }
    }
    fn number(&self) -> Result<f32, ActionError> {
        match self {
            Self::Integer(v) => integer_real(*v),
            Self::Value(Value::Real(v)) => Ok(*v),
            _ => Err(ActionError::Invalid("expected number".into())),
        }
    }
    fn value(self) -> Result<Value, ActionError> {
        match self {
            Self::Value(v) => Ok(v),
            Self::Bool(v) => Ok(Value::Display(v)),
            Self::Integer(v) => integer_real(v).map(Value::Real),
            _ => Err(ActionError::Invalid("expected action parameter".into())),
        }
    }
}
fn integer_real(value: i64) -> Result<f32, ActionError> {
    value
        .to_string()
        .parse()
        .map_err(|_| ActionError::Invalid("numeric parameter".into()))
}
fn narrow_real(value: f64) -> Result<f32, ActionError> {
    let value = value
        .to_string()
        .parse()
        .map_err(|_| ActionError::Invalid("numeric literal".into()))?;
    finite(value)
}
fn finite(value: f32) -> Result<f32, ActionError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ActionError::Invalid("non-finite arithmetic".into()))
    }
}
fn literal(value: &Literal) -> Result<Datum, ActionError> {
    Ok(match value {
        Literal::String(v) => Datum::Value(Value::String(v.clone())),
        Literal::Integer(v) => Datum::Integer(*v),
        Literal::Float(v) => Datum::Value(Value::Real(narrow_real(*v)?)),
        Literal::Bool(v) => Datum::Bool(*v),
        Literal::Unit => Datum::Unit,
    })
}

impl Work<'_> {
    pub(super) fn set_variable(
        &mut self,
        key: &str,
        name: &str,
        value: Datum,
    ) -> Result<(), ActionError> {
        self.text(name)?;
        let slot = (key.to_owned(), name.to_owned());
        let mut bytes = value
            .bytes()
            .saturating_add(name.len())
            .saturating_add(key.len());
        for (other, value) in &self.engine.variables {
            if other != &slot {
                bytes = bytes
                    .saturating_add(other.0.len())
                    .saturating_add(other.1.len())
                    .saturating_add(value.bytes());
            }
        }
        if bytes > self.engine.limits.max_value_bytes {
            return Err(ActionError::Limit("variable bytes"));
        }
        if !self.engine.variables.contains_key(&slot)
            && self.engine.variables.len() >= self.engine.limits.max_entries
        {
            return Err(ActionError::Limit("variables"));
        }
        self.engine.variables.insert(slot, value);
        Ok(())
    }
    pub(super) fn value(
        &mut self,
        value: &Value,
        owner: &Owner,
        depth: usize,
    ) -> Result<Value, ActionError> {
        self.step(depth)?;
        match value {
            Value::Calc(calc) => self.calc(calc, owner, depth + 1),
            Value::Arithmetic(arithmetic) => Ok(Value::Real(self.arithmetic(
                arithmetic,
                owner,
                depth + 1,
            )?)),
            Value::String(v) => {
                self.text(v)?;
                Ok(value.clone())
            }
            Value::Real(v) => {
                finite(*v)?;
                Ok(value.clone())
            }
            Value::Visibility(_)
            | Value::Display(_)
            | Value::LayoutDirection(_)
            | Value::Key(_) => Ok(value.clone()),
        }
    }
    fn arithmetic(
        &mut self,
        value: &Arithmetic,
        owner: &Owner,
        depth: usize,
    ) -> Result<f32, ActionError> {
        self.step(depth)?;
        let (a, b) = match value {
            Arithmetic::Grouping(v) => return self.arithmetic(v, owner, depth + 1),
            Arithmetic::Plus(a, b)
            | Arithmetic::Minus(a, b)
            | Arithmetic::Multiply(a, b)
            | Arithmetic::Divide(a, b)
            | Arithmetic::Min(a, b)
            | Arithmetic::Max(a, b) => (a, b),
        };
        let a = self
            .value(a, owner, depth + 1)?
            .as_f32(None::<&fn(&CalcValue) -> Option<Value>>)
            .ok_or_else(|| ActionError::Invalid("arithmetic operand".into()))?;
        let b = self
            .value(b, owner, depth + 1)?
            .as_f32(None::<&fn(&CalcValue) -> Option<Value>>)
            .ok_or_else(|| ActionError::Invalid("arithmetic operand".into()))?;
        finite(match value {
            Arithmetic::Plus(..) => a + b,
            Arithmetic::Minus(..) => a - b,
            Arithmetic::Multiply(..) => a * b,
            Arithmetic::Divide(..) => a / b,
            Arithmetic::Min(..) => a.min(b),
            Arithmetic::Max(..) => a.max(b),
            Arithmetic::Grouping(_) => unreachable!(),
        })
    }
    fn one(
        &mut self,
        target: &ElementTarget,
        owner: &Owner,
        depth: usize,
    ) -> Result<Vec<usize>, ActionError> {
        let targets = self.targets(target, &owner.source, depth)?;
        // Computed getters follow document-order first-match semantics; mutations target all.
        targets
            .into_iter()
            .next()
            .ok_or_else(|| ActionError::Missing("computed target".into()))
    }
    fn geometry(&self, path: &[usize]) -> Result<Geometry, ActionError> {
        let node = selectors::at(self.root, path).ok_or(ActionError::InvalidSource)?;
        let key = crate::layout::node_key(node, path);
        if let Some(value) = self.event.geometry.get(&key) {
            return Ok(*value);
        }
        Ok(Geometry {
            x: node
                .calculated_x
                .ok_or_else(|| ActionError::Missing("layout x".into()))?,
            y: node
                .calculated_y
                .ok_or_else(|| ActionError::Missing("layout y".into()))?,
            width: node
                .calculated_width
                .ok_or_else(|| ActionError::Missing("layout width".into()))?,
            height: node
                .calculated_height
                .ok_or_else(|| ActionError::Missing("layout height".into()))?,
        })
    }
    fn calc(
        &mut self,
        calc: &CalcValue,
        owner: &Owner,
        depth: usize,
    ) -> Result<Value, ActionError> {
        self.step(depth)?;
        match calc {
            CalcValue::EventValue => self
                .event
                .value
                .clone()
                .map(Value::String)
                .ok_or_else(|| ActionError::Missing("event value".into())),
            CalcValue::Key { key } => Ok(Value::Display(self.event.keys.contains(key))),
            CalcValue::MouseX { target } | CalcValue::MouseY { target } => {
                let (x, y) = self
                    .event
                    .pointer
                    .ok_or_else(|| ActionError::Missing("pointer coordinates".into()))?;
                let position = if let Some(target) = target {
                    let path = self.one(target, owner, depth + 1)?;
                    self.geometry(&path)?
                } else {
                    Geometry::default()
                };
                Ok(Value::Real(finite(
                    if matches!(calc, CalcValue::MouseX { .. }) {
                        x - position.x
                    } else {
                        y - position.y
                    },
                )?))
            }
            CalcValue::Visibility { target }
            | CalcValue::Display { target }
            | CalcValue::Id { target }
            | CalcValue::DataAttrValue { target, .. }
            | CalcValue::WidthPx { target }
            | CalcValue::HeightPx { target }
            | CalcValue::PositionX { target }
            | CalcValue::PositionY { target } => {
                let path = self.one(target, owner, depth + 1)?;
                let node = selectors::at(self.root, &path).ok_or(ActionError::InvalidSource)?;
                let key = crate::layout::node_key(node, &path);
                let style = self
                    .engine
                    .styles
                    .get(&key)
                    .map_or_else(|| Style::read(node), super::StyledNode::effective);
                match calc {
                    CalcValue::Visibility { .. } => {
                        Ok(Value::Visibility(style.visibility.unwrap_or_default()))
                    }
                    CalcValue::Display { .. } => Ok(Value::Display(style.hidden != Some(true))),
                    CalcValue::Id { .. } => node
                        .str_id
                        .clone()
                        .map(Value::String)
                        .ok_or_else(|| ActionError::Missing("string ID".into())),
                    CalcValue::DataAttrValue { attr, .. } => node
                        .data
                        .get(attr)
                        .cloned()
                        .map(Value::String)
                        .ok_or_else(|| ActionError::Missing(format!("data attribute {attr}"))),
                    _ => {
                        let geometry = self.geometry(&path)?;
                        let number = match calc {
                            CalcValue::WidthPx { .. } => geometry.width,
                            CalcValue::HeightPx { .. } => geometry.height,
                            CalcValue::PositionX { .. } => geometry.x,
                            _ => geometry.y,
                        };
                        Ok(Value::Real(finite(number)?))
                    }
                }
            }
        }
    }
    // Exhaustive AST dispatch shares one bounded recursion/step budget across all branches.
    #[allow(clippy::too_many_lines)]
    pub(super) fn expression(
        &mut self,
        expr: &Expression,
        owner: &Owner,
        depth: usize,
    ) -> Result<Datum, ActionError> {
        self.step(depth)?;
        match expr {
            Expression::Literal(v) => {
                if let Literal::String(v) = v {
                    self.text(v)?;
                }
                literal(v)
            }
            Expression::Variable(name) => {
                self.text(name)?;
                match name.as_str() {
                    "event_value" => self
                        .event
                        .value
                        .clone()
                        .map(|v| Datum::Value(Value::String(v)))
                        .ok_or_else(|| ActionError::Missing("event value".into())),
                    "self" => Ok(Datum::Element(owner.source.path.clone())),
                    "Visibility::Hidden" => Ok(Datum::Value(Value::Visibility(Visibility::Hidden))),
                    "Visibility::Visible" => {
                        Ok(Datum::Value(Value::Visibility(Visibility::Visible)))
                    }
                    _ => self
                        .engine
                        .variables
                        .get(&(owner.source.key.clone(), name.clone()))
                        .cloned()
                        .ok_or_else(|| ActionError::Missing(format!("variable {name}"))),
                }
            }
            Expression::Grouping(expr) => self.expression(expr, owner, depth + 1),
            Expression::ElementRef(inner) | Expression::ElementByIdRef(inner) => {
                let text = self.expression(inner, owner, depth + 1)?.string()?;
                let target = if matches!(expr, Expression::ElementByIdRef(_)) {
                    ElementTarget::ById(Target::Literal(text))
                } else {
                    ElementTarget::Selector(Target::Literal(text))
                };
                let paths = self.targets(&target, &owner.source, depth + 1)?;
                Ok(Datum::Element(paths[0].clone()))
            }
            Expression::Array(values) | Expression::Tuple(values) => {
                if values.len() > self.engine.limits.max_entries {
                    return Err(ActionError::Limit("collection"));
                }
                let mut result = Vec::new();
                let mut bytes = 0_usize;
                for value in values {
                    let datum = self.expression(value, owner, depth + 1)?;
                    bytes = bytes
                        .saturating_add(datum.bytes())
                        .saturating_add(std::mem::size_of::<Datum>());
                    if bytes > self.engine.limits.max_value_bytes {
                        return Err(ActionError::Limit("collection bytes"));
                    }
                    result.push(datum);
                }
                Ok(Datum::Array(result))
            }
            Expression::Range {
                start,
                end,
                inclusive,
            } => {
                let start = if let Some(start) = start {
                    self.expression(start, owner, depth + 1)?
                } else {
                    Datum::Integer(0)
                };
                let end = end
                    .as_ref()
                    .ok_or_else(|| ActionError::Unsupported("unbounded range".into()))?;
                let end = self.expression(end, owner, depth + 1)?;
                let (Datum::Integer(start), Datum::Integer(end)) = (start, end) else {
                    return Err(ActionError::Invalid("integer range required".into()));
                };
                let end = if *inclusive {
                    end.checked_add(1).ok_or(ActionError::Limit("range end"))?
                } else {
                    end
                };
                let count = end.saturating_sub(start).max(0);
                if u64::try_from(count).unwrap_or(u64::MAX) > self.engine.limits.max_entries as u64
                {
                    return Err(ActionError::Limit("range length"));
                }
                Ok(Datum::Array((start..end).map(Datum::Integer).collect()))
            }
            Expression::Unary { op, expr } => {
                let value = self.expression(expr, owner, depth + 1)?;
                match op {
                    UnaryOp::Not => Ok(Datum::Bool(!value.bool()?)),
                    UnaryOp::Minus => match value {
                        Datum::Integer(v) => Ok(Datum::Integer(
                            v.checked_neg()
                                .ok_or(ActionError::Limit("integer overflow"))?,
                        )),
                        v => Ok(Datum::Value(Value::Real(finite(-v.number()?)?))),
                    },
                    UnaryOp::Plus => {
                        value.number()?;
                        Ok(value)
                    }
                    UnaryOp::Ref => Ok(value),
                }
            }
            Expression::Binary { left, op, right } => {
                let left = self.expression(left, owner, depth + 1)?;
                if *op == BinaryOp::And && !left.bool()? {
                    return Ok(Datum::Bool(false));
                }
                if *op == BinaryOp::Or && left.bool()? {
                    return Ok(Datum::Bool(true));
                }
                let right = self.expression(right, owner, depth + 1)?;
                self.binary(&left, op, &right)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if self.expression(condition, owner, depth + 1)?.bool()? {
                    self.expression(then_branch, owner, depth + 1)
                } else if let Some(branch) = else_branch {
                    self.expression(branch, owner, depth + 1)
                } else {
                    Ok(Datum::Unit)
                }
            }
            Expression::Block(block) => self.block(block, owner, depth + 1),
            Expression::Match { expr, arms } => {
                let value = self.expression(expr, owner, depth + 1)?;
                for arm in arms {
                    if self.pattern(&arm.pattern, &value, owner)? {
                        return self.expression(&arm.body, owner, depth + 1);
                    }
                }
                Err(ActionError::Invalid("non-exhaustive match".into()))
            }
            Expression::Call { function, args } => self.call(function, args, owner, depth + 1),
            Expression::MethodCall {
                receiver,
                method,
                args,
            } => {
                let receiver = self.expression(receiver, owner, depth + 1)?;
                self.method(receiver, method, args, owner, depth + 1)
            }
            Expression::Field { object, field } => {
                if matches!(object.as_ref(), Expression::Variable(name) if name == "http") {
                    return self.http_field(field);
                }
                let object = self.expression(object, owner, depth + 1)?;
                match object {
                    Datum::Element(path) => self.element_field(&path, field, owner, depth + 1),
                    _ => Err(ActionError::Unsupported(format!("field {field}"))),
                }
            }
            Expression::Closure { .. } => Err(ActionError::Unsupported(
                "runtime closures require a registered host function".into(),
            )),
            Expression::RawRust(_) => Err(ActionError::Unsupported(
                "RawRust cannot be executed by a terminal renderer".into(),
            )),
        }
    }
    fn binary(&self, left: &Datum, op: &BinaryOp, right: &Datum) -> Result<Datum, ActionError> {
        use BinaryOp as B;
        match op {
            B::Equal => return Ok(Datum::Bool(left == right)),
            B::NotEqual => return Ok(Datum::Bool(left != right)),
            B::And => return Ok(Datum::Bool(left.bool()? && right.bool()?)),
            B::Or => return Ok(Datum::Bool(left.bool()? || right.bool()?)),
            B::Add if matches!(left, Datum::Value(Value::String(_))) => {
                let text = left.string()? + &right.string()?;
                self.text(&text)?;
                return Ok(Datum::Value(Value::String(text)));
            }
            _ => {}
        }
        if let (Datum::Integer(a), Datum::Integer(b)) = (left, right) {
            let value = match op {
                B::Add => a.checked_add(*b),
                B::Subtract => a.checked_sub(*b),
                B::Multiply => a.checked_mul(*b),
                B::Divide => a.checked_div(*b),
                B::Modulo => a.checked_rem(*b),
                B::BitAnd => Some(a & b),
                B::BitOr => Some(a | b),
                B::BitXor => Some(a ^ b),
                B::Less => return Ok(Datum::Bool(a < b)),
                B::LessEqual => return Ok(Datum::Bool(a <= b)),
                B::Greater => return Ok(Datum::Bool(a > b)),
                B::GreaterEqual => return Ok(Datum::Bool(a >= b)),
                _ => return Err(ActionError::Invalid("binary operands".into())),
            };
            return value
                .map(Datum::Integer)
                .ok_or_else(|| ActionError::Invalid("integer overflow/division by zero".into()));
        }
        let a = left.number()?;
        let b = right.number()?;
        let result = match op {
            B::Add => a + b,
            B::Subtract => a - b,
            B::Multiply => a * b,
            B::Divide => a / b,
            B::Modulo => a % b,
            B::Less => return Ok(Datum::Bool(a < b)),
            B::LessEqual => return Ok(Datum::Bool(a <= b)),
            B::Greater => return Ok(Datum::Bool(a > b)),
            B::GreaterEqual => return Ok(Datum::Bool(a >= b)),
            _ => return Err(ActionError::Invalid("binary operands".into())),
        };
        Ok(Datum::Value(Value::Real(finite(result)?)))
    }
    fn pattern(
        &mut self,
        pattern: &Pattern,
        value: &Datum,
        owner: &Owner,
    ) -> Result<bool, ActionError> {
        match pattern {
            Pattern::Wildcard => Ok(true),
            Pattern::Literal(v) => Ok(literal(v)? == *value),
            Pattern::Variable(name) => {
                self.set_variable(&owner.source.key, name, value.clone())?;
                Ok(true)
            }
            Pattern::Variant {
                enum_name,
                variant,
                fields,
            } => {
                if !fields.is_empty() {
                    return Err(ActionError::Unsupported("structured enum pattern".into()));
                }
                match (enum_name.as_str(), variant.as_str(), value) {
                    ("Visibility", "Hidden", Datum::Value(Value::Visibility(v))) => {
                        Ok(*v == Visibility::Hidden)
                    }
                    ("Visibility", "Visible", Datum::Value(Value::Visibility(v))) => {
                        Ok(*v == Visibility::Visible)
                    }
                    _ => Err(ActionError::Unsupported("enum pattern".into())),
                }
            }
        }
    }
    fn block(&mut self, block: &Block, owner: &Owner, depth: usize) -> Result<Datum, ActionError> {
        self.step(depth)?;
        let mut value = Datum::Unit;
        for statement in &block.statements {
            if let Statement::Expression(expr) = statement {
                value = self.expression(expr, owner, depth + 1)?;
            } else {
                self.statement(statement, owner, depth + 1)?;
                value = Datum::Unit;
            }
        }
        Ok(value)
    }
    fn statement(
        &mut self,
        statement: &Statement,
        owner: &Owner,
        depth: usize,
    ) -> Result<(), ActionError> {
        self.step(depth)?;
        match statement {
            Statement::Expression(expr) => {
                self.expression(expr, owner, depth + 1)?;
            }
            Statement::Let { name, value } => {
                let value = self.expression(value, owner, depth + 1)?;
                self.set_variable(&owner.source.key, name, value)?;
            }
            Statement::Block(block) => {
                self.block(block, owner, depth + 1)?;
            }
            Statement::If {
                condition,
                then_block,
                else_block,
            } => {
                if self.expression(condition, owner, depth + 1)?.bool()? {
                    self.block(then_block, owner, depth + 1)?;
                } else if let Some(block) = else_block {
                    self.block(block, owner, depth + 1)?;
                }
            }
            Statement::For {
                pattern,
                iter,
                body,
            } => {
                let Datum::Array(values) = self.expression(iter, owner, depth + 1)? else {
                    return Err(ActionError::Invalid("for expects collection".into()));
                };
                for value in values {
                    self.step(depth + 1)?;
                    self.set_variable(&owner.source.key, pattern, value)?;
                    self.block(body, owner, depth + 1)?;
                }
            }
            Statement::While { condition, body } => {
                while self.expression(condition, owner, depth + 1)?.bool()? {
                    self.step(depth + 1)?;
                    self.block(body, owner, depth + 1)?;
                }
            }
            Statement::Match { expr, arms } => {
                let value = self.expression(expr, owner, depth + 1)?;
                let mut matched = false;
                for arm in arms {
                    if self.pattern(&arm.pattern, &value, owner)? {
                        self.expression(&arm.body, owner, depth + 1)?;
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return Err(ActionError::Invalid("non-exhaustive match".into()));
                }
            }
        }
        Ok(())
    }
    fn http_field(&self, field: &str) -> Result<Datum, ActionError> {
        let http = self
            .event
            .http
            .as_ref()
            .ok_or(ActionError::MissingHttpContext)?;
        let value = match field {
            "url" => Datum::Value(Value::String(http.url.clone())),
            "method" => Datum::Value(Value::String(http.method.clone())),
            "status" => http
                .status
                .map_or(Datum::Unit, |v| Datum::Integer(i64::from(v))),
            "duration_ms" => http
                .duration_ms
                .map(|v| {
                    i64::try_from(v)
                        .map(Datum::Integer)
                        .map_err(|_| ActionError::Limit("duration"))
                })
                .transpose()?
                .unwrap_or(Datum::Unit),
            "error" => http
                .error
                .clone()
                .map_or(Datum::Unit, |v| Datum::Value(Value::String(v))),
            "headers" => Datum::Array(
                http.headers
                    .iter()
                    .flat_map(|v| v.iter())
                    .map(|(k, v)| {
                        Datum::Array(vec![
                            Datum::Value(Value::String(k.clone())),
                            Datum::Value(Value::String(v.clone())),
                        ])
                    })
                    .collect(),
            ),
            _ => return Err(ActionError::Missing(format!("HTTP field {field}"))),
        };
        Ok(value)
    }
    fn element_field(
        &mut self,
        path: &[usize],
        field: &str,
        owner: &Owner,
        depth: usize,
    ) -> Result<Datum, ActionError> {
        selectors::at(self.root, path).ok_or(ActionError::InvalidSource)?;
        let mut target_owner = owner.clone();
        target_owner.source = self.source(path)?;
        let target = ElementTarget::SelfTarget;
        let calc = match field {
            "id" => CalcValue::Id { target },
            "visibility" => CalcValue::Visibility { target },
            "display" => CalcValue::Display { target },
            "width" => CalcValue::WidthPx { target },
            "height" => CalcValue::HeightPx { target },
            "x" => CalcValue::PositionX { target },
            "y" => CalcValue::PositionY { target },
            _ => return Err(ActionError::Unsupported(format!("element field {field}"))),
        };
        Ok(Datum::Value(self.calc(&calc, &target_owner, depth)?))
    }
    fn call(
        &mut self,
        function: &str,
        args: &[Expression],
        owner: &Owner,
        depth: usize,
    ) -> Result<Datum, ActionError> {
        if args.len() > self.engine.limits.max_entries {
            return Err(ActionError::Limit("call arguments"));
        }
        if function == "event_value" && args.is_empty() {
            return Ok(Datum::Value(self.calc(
                &CalcValue::EventValue,
                owner,
                depth,
            )?));
        }
        if matches!(function, "element" | "element_by_id") && args.len() == 1 {
            let text = self.expression(&args[0], owner, depth + 1)?.string()?;
            let target = if function == "element_by_id" {
                ElementTarget::ById(Target::Literal(text))
            } else {
                ElementTarget::Selector(Target::Literal(text))
            };
            let path = self.one(&target, owner, depth + 1)?;
            return Ok(Datum::Element(path));
        }
        if matches!(function, "min" | "max") && args.len() == 2 {
            let a = self.expression(&args[0], owner, depth + 1)?.number()?;
            let b = self.expression(&args[1], owner, depth + 1)?.number()?;
            return Ok(Datum::Value(Value::Real(if function == "min" {
                a.min(b)
            } else {
                a.max(b)
            })));
        }
        Err(ActionError::Unsupported(format!("function {function}")))
    }
    fn method(
        &mut self,
        receiver: Datum,
        method: &str,
        args: &[Expression],
        owner: &Owner,
        depth: usize,
    ) -> Result<Datum, ActionError> {
        self.step(depth)?;
        if method == "len" && args.is_empty() {
            let length = match receiver {
                Datum::Array(v) => v.len(),
                Datum::Value(Value::String(v)) => v.chars().count(),
                _ => return Err(ActionError::Invalid("len receiver".into())),
            };
            return Ok(Datum::Integer(
                i64::try_from(length).map_err(|_| ActionError::Limit("length"))?,
            ));
        }
        if method == "clone" && args.is_empty() {
            return Ok(receiver);
        }
        let Datum::Element(path) = receiver else {
            return Err(ActionError::Unsupported(format!("method {method}")));
        };
        if args.is_empty()
            && matches!(
                method,
                "id" | "visibility" | "display" | "width" | "height" | "x" | "y"
            )
        {
            return self.element_field(&path, method, owner, depth + 1);
        }
        let mut target_owner = owner.clone();
        target_owner.source = self.source(&path)?;
        let target = ElementTarget::SelfTarget;
        let action = match (method, args.len()) {
            ("show", 0) => hyperchad_actions::StyleAction::SetDisplay(true),
            ("hide", 0) => hyperchad_actions::StyleAction::SetDisplay(false),
            ("focus", 0) => hyperchad_actions::StyleAction::SetFocus(true),
            ("blur", 0) => hyperchad_actions::StyleAction::SetFocus(false),
            ("set_display", 1) => hyperchad_actions::StyleAction::SetDisplay(
                self.expression(&args[0], owner, depth + 1)?.bool()?,
            ),
            ("set_background", 1) => hyperchad_actions::StyleAction::SetBackground(Some(
                self.expression(&args[0], owner, depth + 1)?.string()?,
            )),
            ("remove_background", 0) => hyperchad_actions::StyleAction::SetBackground(None),
            ("set_visibility", 1) => match self.expression(&args[0], owner, depth + 1)?.value()? {
                Value::Visibility(v) => hyperchad_actions::StyleAction::SetVisibility(v),
                _ => return Err(ActionError::Invalid("visibility value".into())),
            },
            _ => return Err(ActionError::Unsupported(format!("element method {method}"))),
        };
        self.style(&target, &action, &target_owner, 0)?;
        Ok(Datum::Unit)
    }
}

/// Validate retained action/expression ASTs before cloning them into registrations.
pub(super) fn validate_action(
    action: &ActionType,
    limits: &ActionLimits,
) -> Result<(), ActionError> {
    super::admission::action(action, limits)?;
    let mut stack = vec![(action, 0_usize)];
    let mut count = 0_usize;
    while let Some((action, depth)) = stack.pop() {
        count = count.saturating_add(1);
        if count > limits.max_steps || depth > limits.max_depth {
            return Err(ActionError::Limit("registered action AST"));
        }
        match action {
            ActionType::Event { action, .. } | ActionType::Parameterized { action, .. } => {
                stack.push((action, depth + 1));
            }
            ActionType::Multi(actions) => {
                if actions.len() > limits.max_entries {
                    return Err(ActionError::Limit("registered actions"));
                }
                for action in actions {
                    stack.push((action, depth + 1));
                }
            }
            ActionType::MultiEffect(effects) => {
                if effects.len() > limits.max_entries {
                    return Err(ActionError::Limit("registered effects"));
                }
                for effect in effects {
                    stack.push((&effect.action, depth + 1));
                }
            }
            ActionType::Logic(condition) => {
                for effect in condition.actions.iter().chain(&condition.else_actions) {
                    stack.push((&effect.action, depth + 1));
                }
            }
            ActionType::NoOp
            | ActionType::Let { .. }
            | ActionType::Style { .. }
            | ActionType::Input(_)
            | ActionType::Navigate { .. }
            | ActionType::Log { .. }
            | ActionType::Custom { .. } => {}
        }
    }
    Ok(())
}
