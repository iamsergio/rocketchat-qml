// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Minimal XML-RPC client: the value types and calls the Spix protocol uses, over plain HTTP.

use std::fmt::Write as _;
use std::time::Duration;

/// An XML-RPC value. Untyped `<value>text</value>` decodes as a string, which is also how Spix
/// answers calls that return nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i32),
    Double(f64),
    String(String),
    Array(Vec<Value>),
    Struct(Vec<(String, Value)>),
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::String(s.to_owned())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP request failed")]
    Http(#[from] Box<ureq::Error>),
    #[error("server fault {code}: {message}")]
    Fault { code: i32, message: String },
    #[error("malformed XML-RPC response")]
    Xml(#[from] roxmltree::Error),
    #[error("malformed XML-RPC response: {0}")]
    Malformed(String),
}

impl From<ureq::Error> for Error {
    fn from(error: ureq::Error) -> Self {
        Self::Http(Box::new(error))
    }
}

/// A connection to one XML-RPC endpoint.
pub struct Client {
    agent: ureq::Agent,
    url: String,
}

impl Client {
    /// Talks to `http://127.0.0.1:<port>/RPC2`. A call that takes longer than `timeout` fails.
    pub fn new(port: u16, timeout: Duration) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .build()
            .into();
        Self {
            agent,
            url: format!("http://127.0.0.1:{port}/RPC2"),
        }
    }

    pub fn call(&self, method: &str, params: &[Value]) -> Result<Value, Error> {
        let body = self
            .agent
            .post(&self.url)
            .header("Content-Type", "text/xml")
            .send(encode_call(method, params))?
            .body_mut()
            .read_to_string()?;
        decode_response(&body)
    }
}

fn encode_call(method: &str, params: &[Value]) -> String {
    let mut xml = String::from(r#"<?xml version="1.0"?><methodCall><methodName>"#);
    escape_into(&mut xml, method);
    xml.push_str("</methodName><params>");
    for param in params {
        xml.push_str("<param>");
        encode_value(&mut xml, param);
        xml.push_str("</param>");
    }
    xml.push_str("</params></methodCall>");
    xml
}

fn encode_value(xml: &mut String, value: &Value) {
    xml.push_str("<value>");
    match value {
        Value::Bool(b) => {
            let _ = write!(xml, "<boolean>{}</boolean>", u8::from(*b));
        }
        Value::Int(i) => {
            let _ = write!(xml, "<int>{i}</int>");
        }
        Value::Double(d) => {
            let _ = write!(xml, "<double>{d}</double>");
        }
        Value::String(s) => {
            xml.push_str("<string>");
            escape_into(xml, s);
            xml.push_str("</string>");
        }
        Value::Array(values) => {
            xml.push_str("<array><data>");
            for v in values {
                encode_value(xml, v);
            }
            xml.push_str("</data></array>");
        }
        Value::Struct(members) => {
            xml.push_str("<struct>");
            for (name, v) in members {
                xml.push_str("<member><name>");
                escape_into(xml, name);
                xml.push_str("</name>");
                encode_value(xml, v);
                xml.push_str("</member>");
            }
            xml.push_str("</struct>");
        }
    }
    xml.push_str("</value>");
}

fn escape_into(xml: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => xml.push_str("&amp;"),
            '<' => xml.push_str("&lt;"),
            '>' => xml.push_str("&gt;"),
            _ => xml.push(c),
        }
    }
}

fn decode_response(xml: &str) -> Result<Value, Error> {
    let doc = roxmltree::Document::parse(xml)?;
    let root = doc.root_element();
    if !root.has_tag_name("methodResponse") {
        return Err(malformed(&root));
    }
    let body = only_child(&root)?;
    match body.tag_name().name() {
        "params" => {
            let param = only_child(&body)?;
            if !param.has_tag_name("param") {
                return Err(malformed(&param));
            }
            decode_value(&only_child(&param)?)
        }
        "fault" => decode_fault(&decode_value(&only_child(&body)?)?),
        _ => Err(malformed(&body)),
    }
}

fn decode_fault(fault: &Value) -> Result<Value, Error> {
    let Value::Struct(members) = fault else {
        return Err(Error::Malformed(format!(
            "fault is not a struct: {fault:?}"
        )));
    };
    let member = |name: &str| members.iter().find(|(n, _)| n == name).map(|(_, v)| v);
    match (member("faultCode"), member("faultString")) {
        (Some(Value::Int(code)), Some(Value::String(message))) => Err(Error::Fault {
            code: *code,
            message: message.clone(),
        }),
        _ => Err(Error::Malformed(format!("unexpected fault: {fault:?}"))),
    }
}

fn decode_value(node: &roxmltree::Node) -> Result<Value, Error> {
    if !node.has_tag_name("value") {
        return Err(malformed(node));
    }
    let Some(typed) = node.children().find(roxmltree::Node::is_element) else {
        return Ok(Value::String(node.text().unwrap_or_default().to_owned()));
    };
    let text = typed.text().unwrap_or_default();
    let value = match typed.tag_name().name() {
        "string" => Value::String(text.to_owned()),
        "boolean" => match text {
            "0" => Value::Bool(false),
            "1" => Value::Bool(true),
            _ => return Err(malformed(&typed)),
        },
        "int" | "i4" => Value::Int(text.trim().parse().map_err(|_| malformed(&typed))?),
        "double" => Value::Double(text.trim().parse().map_err(|_| malformed(&typed))?),
        "array" => {
            let data = only_child(&typed)?;
            if !data.has_tag_name("data") {
                return Err(malformed(&data));
            }
            Value::Array(
                elements(&data)
                    .map(|v| decode_value(&v))
                    .collect::<Result<_, _>>()?,
            )
        }
        "struct" => Value::Struct(
            elements(&typed)
                .map(|m| decode_member(&m))
                .collect::<Result<_, _>>()?,
        ),
        _ => return Err(malformed(&typed)),
    };
    Ok(value)
}

fn decode_member(member: &roxmltree::Node) -> Result<(String, Value), Error> {
    let mut children = elements(member);
    match (children.next(), children.next(), children.next()) {
        (Some(name), Some(value), None)
            if member.has_tag_name("member") && name.has_tag_name("name") =>
        {
            Ok((
                name.text().unwrap_or_default().to_owned(),
                decode_value(&value)?,
            ))
        }
        _ => Err(malformed(member)),
    }
}

fn elements<'a, 'input>(
    node: &roxmltree::Node<'a, 'input>,
) -> impl Iterator<Item = roxmltree::Node<'a, 'input>> {
    node.children().filter(roxmltree::Node::is_element)
}

fn only_child<'a, 'input>(
    node: &roxmltree::Node<'a, 'input>,
) -> Result<roxmltree::Node<'a, 'input>, Error> {
    let mut children = elements(node);
    match (children.next(), children.next()) {
        (Some(child), None) => Ok(child),
        _ => Err(malformed(node)),
    }
}

fn malformed(node: &roxmltree::Node) -> Error {
    Error::Malformed(format!("unexpected <{}>", node.tag_name().name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(params: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8" ?><methodResponse><params><param>{params}</param></params></methodResponse>"#
        )
    }

    // INFRA-001
    #[test]
    fn encodes_a_call_with_escaped_strings() {
        let xml = encode_call(
            "inputText",
            &[
                Value::String("a/b".into()),
                Value::String("<&> ü".into()),
                Value::Int(-3),
                Value::Bool(true),
            ],
        );
        assert_eq!(
            xml,
            "<?xml version=\"1.0\"?><methodCall><methodName>inputText</methodName><params>\
             <param><value><string>a/b</string></value></param>\
             <param><value><string>&lt;&amp;&gt; ü</string></value></param>\
             <param><value><int>-3</int></value></param>\
             <param><value><boolean>1</boolean></value></param>\
             </params></methodCall>"
        );
    }

    // INFRA-001
    #[test]
    fn encoded_values_decode_to_themselves() {
        let value = Value::Array(vec![
            Value::Bool(false),
            Value::Int(42),
            Value::Double(640.5),
            Value::String("x &lt; y".into()),
            Value::Struct(vec![("k".into(), Value::Array(vec![]))]),
        ]);
        let mut xml = String::new();
        encode_value(&mut xml, &value);
        assert_eq!(decode_response(&response(&xml)).unwrap(), value);
    }

    // INFRA-001
    #[test]
    fn decodes_what_spix_sends() {
        // Captured from Spix's AnyRPC server.
        let decode = |value: &str| decode_response(&response(value)).unwrap();
        assert_eq!(decode("<value></value>"), Value::String(String::new()));
        assert_eq!(
            decode("<value>a &lt;b&gt; &amp; ü</value>"),
            Value::String("a <b> & ü".into())
        );
        assert_eq!(
            decode("<value><boolean>1</boolean></value>"),
            Value::Bool(true)
        );
        assert_eq!(
            decode(
                "<value><array><data><value><double>2</double></value><value><double>640.00000000000005</double></value></data></array></value>"
            ),
            Value::Array(vec![Value::Double(2.0), Value::Double(640.0)])
        );
        assert_eq!(
            decode("<value><array><data></data></array></value>"),
            Value::Array(vec![])
        );
    }

    // INFRA-001
    #[test]
    fn decodes_a_fault_into_an_error() {
        let xml = r#"<?xml version="1.0" encoding="utf-8" ?><methodResponse><fault><value><struct><member><name>faultCode</name><value><i4>-32601</i4></value></member><member><name>faultString</name><value>Method not found</value></member></struct></value></fault></methodResponse>"#;
        match decode_response(xml) {
            Err(Error::Fault { code, message }) => {
                assert_eq!(code, -32601);
                assert_eq!(message, "Method not found");
            }
            other => panic!("expected a fault, got {other:?}"),
        }
    }

    // INFRA-001
    #[test]
    fn rejects_malformed_responses() {
        for xml in [
            "not xml",
            "<methodResponse/>",
            "<other><params/></other>",
            &response("<value><boolean>2</boolean></value>"),
            &response("<value><int>x</int></value>"),
            &response("<value><dateTime.iso8601>20260101T00:00:00</dateTime.iso8601></value>"),
            "<methodResponse><fault><value>oops</value></fault></methodResponse>",
        ] {
            assert!(decode_response(xml).is_err(), "accepted {xml}");
        }
    }
}
