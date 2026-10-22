use anyhow::{bail, Context, Result};
use chrono::{Local, SecondsFormat, Utc};
use rust_decimal::{prelude::ToPrimitive, Decimal, MathematicalOps, RoundingStrategy};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeArgs {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CalculateArgs {
    expression: String,
}

pub fn get_time(arguments: Value) -> Result<Value> {
    let _: TimeArgs = serde_json::from_value(arguments).context("getTime 不接受参数")?;
    let now = Local::now();
    Ok(json!({
        "local_time": now.to_rfc3339_opts(SecondsFormat::Millis, false),
        "utc_time": now.with_timezone(&Utc).to_rfc3339_opts(SecondsFormat::Millis, true),
        "timezone_offset": now.format("%:z").to_string(),
        "unix_ms": now.timestamp_millis(),
    }))
}

pub fn calculate(arguments: Value) -> Result<Value> {
    let args: CalculateArgs = serde_json::from_value(arguments).context("calculate 参数无效")?;
    if args.expression.trim().is_empty() || args.expression.len() > 2000 {
        bail!("计算表达式不能为空或超过 2000 字节")
    }
    let mut parser = Calculator {
        rest: &args.expression,
        depth: 0,
    };
    let value = parser.expression()?;
    if parser.peek().is_some() {
        bail!("表达式包含无法识别的内容，请使用数字、算术运算符、括号或支持的数学函数")
    }
    Ok(json!({
        "expression": args.expression,
        "result": value.normalize().to_string(),
        "max_fractional_digits": Decimal::MAX_SCALE,
    }))
}

// Parse arithmetic only; model input is never passed to a shell or code evaluator.
struct Calculator<'a> {
    rest: &'a str,
    depth: usize,
}

impl Calculator<'_> {
    fn peek(&mut self) -> Option<u8> {
        self.rest = self.rest.trim_start();
        self.rest.as_bytes().first().copied()
    }

    fn expression(&mut self) -> Result<Decimal> {
        let mut value = self.term()?;
        while let Some(operator @ (b'+' | b'-')) = self.peek() {
            self.rest = &self.rest[1..];
            let right = self.term()?;
            value = match operator {
                b'+' => value.checked_add(right),
                _ => value.checked_sub(right),
            }
            .context("计算结果超出十进制支持范围")?;
        }
        Ok(value)
    }

    fn term(&mut self) -> Result<Decimal> {
        let mut value = self.factor()?;
        while let Some(operator @ (b'*' | b'/' | b'%')) = self.peek() {
            self.rest = &self.rest[1..];
            let right = self.factor()?;
            if operator != b'*' && right.is_zero() {
                bail!("除数不能为 0")
            }
            value = match operator {
                b'*' => value.checked_mul(right),
                b'/' => value.checked_div(right),
                _ => value.checked_rem(right),
            }
            .context("计算结果超出十进制支持范围")?;
        }
        Ok(value)
    }

    fn factor(&mut self) -> Result<Decimal> {
        if self.depth >= 64 {
            bail!("表达式嵌套不能超过 64 层")
        }
        self.depth += 1;
        let value = match self.peek() {
            Some(b'+' | b'-') => {
                let negative = self.rest.starts_with('-');
                self.rest = &self.rest[1..];
                let value = self.factor()?;
                if negative {
                    -value
                } else {
                    value
                }
            }
            _ => self.power()?,
        };
        self.depth -= 1;
        Ok(value)
    }

    fn power(&mut self) -> Result<Decimal> {
        let value = self.primary()?;
        self.peek();
        if self.rest.starts_with('^') || self.rest.starts_with("**") {
            self.rest = &self.rest[if self.rest.starts_with('^') { 1 } else { 2 }..];
            let exponent = self.factor()?;
            Self::raise(value, exponent)
        } else {
            Ok(value)
        }
    }

    fn raise(base: Decimal, exponent: Decimal) -> Result<Decimal> {
        if base.is_zero() && exponent < Decimal::ZERO {
            bail!("0 不能取负指数")
        }
        if exponent.fract().is_zero() {
            return base
                .checked_powd(exponent)
                .context("乘方结果超出十进制支持范围");
        }
        if base < Decimal::ZERO {
            bail!("负数的非整数次幂不在当前实数计算范围内")
        }
        if exponent == Decimal::new(5, 1) {
            return base.sqrt().context("平方根的参数不能为负数");
        }
        if base.is_zero() {
            return Ok(Decimal::ZERO);
        }
        base.checked_ln()
            .and_then(|value| value.checked_mul(exponent))
            .and_then(|value| value.checked_exp_with_tolerance(Decimal::new(1, 26)))
            .context("乘方结果超出十进制支持范围")
    }

    fn primary(&mut self) -> Result<Decimal> {
        match self.peek() {
            Some(b'(') => {
                self.rest = &self.rest[1..];
                let value = self.expression()?;
                if self.peek() != Some(b')') {
                    bail!("计算表达式缺少右括号")
                }
                self.rest = &self.rest[1..];
                Ok(value)
            }
            Some(b'0'..=b'9' | b'.') => {
                let length = self
                    .rest
                    .bytes()
                    .take_while(|c| c.is_ascii_digit() || *c == b'.')
                    .count();
                let mut length = length;
                if matches!(self.rest.as_bytes().get(length), Some(b'e' | b'E')) {
                    length += 1;
                    if matches!(self.rest.as_bytes().get(length), Some(b'+' | b'-')) {
                        length += 1;
                    }
                    length += self.rest.as_bytes()[length..]
                        .iter()
                        .take_while(|c| c.is_ascii_digit())
                        .count();
                }
                let number = &self.rest[..length];
                self.rest = &self.rest[length..];
                if number.contains(['e', 'E']) {
                    Decimal::from_scientific(number)
                } else {
                    Decimal::from_str_exact(number)
                }
                .context("十进制数字无效或超出支持精度")
            }
            Some(b'a'..=b'z' | b'A'..=b'Z') => {
                let length = self
                    .rest
                    .bytes()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == b'_')
                    .count();
                let name = self.rest[..length].to_ascii_lowercase();
                self.rest = &self.rest[length..];
                let mut args = Vec::new();
                if self.peek() == Some(b'(') {
                    self.rest = &self.rest[1..];
                    if self.peek() != Some(b')') {
                        loop {
                            args.push(self.expression()?);
                            if self.peek() != Some(b',') {
                                break;
                            }
                            self.rest = &self.rest[1..];
                        }
                    }
                    if self.peek() != Some(b')') {
                        bail!("函数调用缺少右括号");
                    }
                    self.rest = &self.rest[1..];
                } else if name != "pi" && name != "e" {
                    bail!("函数调用需要括号；不支持变量：{name}")
                }
                Self::function(&name, &args)
            }
            _ => bail!("此处需要数字、括号或数学函数"),
        }
    }

    fn function(name: &str, args: &[Decimal]) -> Result<Decimal> {
        let result = match (name, args) {
            ("pi", []) => Some(Decimal::PI),
            ("e", []) => Some(Decimal::E),
            ("sqrt", [x]) => {
                return x.sqrt().context("平方根的参数不能为负数");
            }
            ("pow", [base, exponent]) => return Self::raise(*base, *exponent),
            ("abs", [x]) => Some(x.abs()),
            ("floor", [x]) => Some(x.floor()),
            ("ceil", [x]) => Some(x.ceil()),
            ("trunc" | "int", [x]) => Some(x.trunc()),
            ("sign", [x]) => Some(if x.is_zero() {
                Decimal::ZERO
            } else if *x < Decimal::ZERO {
                -Decimal::ONE
            } else {
                Decimal::ONE
            }),
            ("round", [x]) => {
                Some(x.round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero))
            }
            ("round", [x, digits]) => {
                if !digits.fract().is_zero()
                    || *digits < Decimal::ZERO
                    || *digits > Decimal::from(28)
                {
                    bail!("round 的小数位数必须是 0 到 28 的整数")
                }
                Some(x.round_dp_with_strategy(
                    digits.to_u32().unwrap(),
                    RoundingStrategy::MidpointAwayFromZero,
                ))
            }
            ("min", values) if !values.is_empty() => values.iter().copied().min(),
            ("max", values) if !values.is_empty() => values.iter().copied().max(),
            ("ln", [x]) => {
                if *x <= Decimal::ZERO {
                    bail!("对数的参数必须大于 0");
                }
                x.checked_ln()
            }
            ("log" | "log10", [x]) => {
                if *x <= Decimal::ZERO {
                    bail!("对数的参数必须大于 0");
                }
                x.checked_log10()
            }
            ("log", [base, x]) => {
                if *base <= Decimal::ZERO || *base == Decimal::ONE || *x <= Decimal::ZERO {
                    bail!("log(base, x) 的底数必须大于 0 且不等于 1，x 必须大于 0")
                }
                x.checked_ln()
                    .zip(base.checked_ln())
                    .and_then(|(x, base)| x.checked_div(base))
            }
            ("exp", [x]) => x.checked_exp_with_tolerance(Decimal::new(1, 26)),
            ("sin", [x]) => x.checked_sin(),
            ("cos", [x]) => x.checked_cos(),
            ("tan", [x]) => x.checked_tan(),
            ("radians", [x]) => x
                .checked_div(Decimal::from(180))
                .and_then(|value| value.checked_mul(Decimal::PI)),
            ("degrees", [x]) => x
                .checked_div(Decimal::PI)
                .and_then(|value| value.checked_mul(Decimal::from(180))),
            _ => bail!(
                "不支持的数学函数或参数数量：{name}（{} 个参数）",
                args.len()
            ),
        };
        result.context("函数结果超出十进制支持范围或不在定义域内")
    }
}
