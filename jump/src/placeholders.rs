// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

#[cfg_attr(test, derive(Eq, PartialEq))]
#[derive(Debug)]
pub(crate) enum Placeholder<'a> {
    FileName(&'a str),
    Scie,
    ScieBindings,
    ScieBindingCmd(&'a str),
    ScieLift,
    Env(&'a str),
}

#[cfg_attr(test, derive(Eq, PartialEq))]
#[derive(Debug)]
pub(crate) enum Item<'a> {
    Text(&'a str),
    LeftBrace,
    Placeholder(Placeholder<'a>),
}

#[cfg_attr(test, derive(Eq, PartialEq))]
#[derive(Debug)]
pub(crate) struct Parsed<'a> {
    pub items: Vec<Item<'a>>,
}

pub(crate) fn parse(text: &str) -> Result<Parsed, String> {
    let mut items = vec![];

    let mut previous_char: Option<char> = None;
    let mut inside_placeholder = false;
    let mut start = 0_usize;

    if "{" == text {
        return Err(
            "Encountered text of '{'. If a literal '{' is intended, escape it like so: '{{'."
                .to_string(),
        );
    }
    for (index, current_char) in text.chars().enumerate() {
        match current_char {
            '{' if !inside_placeholder => {
                if index - start > 0 {
                    items.push(Item::Text(&text[start..index]))
                }
                previous_char = Some('{');
                inside_placeholder = true;
                start = index + 1;
            }
            '{' if inside_placeholder && Some('{') == previous_char => {
                items.push(Item::LeftBrace);
                inside_placeholder = false;
                start = index + 1;
            }
            '{' if inside_placeholder => {
                return Err(format!(
                    "Encountered '{{' at character {pos} inside placeholder starting at character \
                    {placeholder_pos} in {text}'. Placeholders symbols cannot include the '{{' or \
                    '}}' characters",
                    pos = index + 1,
                    placeholder_pos = start,
                ));
            }
            '}' if inside_placeholder => {
                let symbol = &text[start..index];
                if symbol.is_empty() {
                    return Err(format!(
                        "Encountered placeholder '{{}}' at {pos} in '{text}'. Placeholders must \
                        have names. If A literal '{{}}' is intended, escape the opening bracket \
                        like so '{{{{}}'.",
                        pos = index - 1
                    ));
                }
                match symbol.splitn(3, '.').collect::<Vec<_>>()[..] {
                    ["scie"] => items.push(Item::Placeholder(Placeholder::Scie)),
                    ["scie", "bindings"] => {
                        items.push(Item::Placeholder(Placeholder::ScieBindings))
                    }
                    ["scie", "bindings", cmd] => {
                        items.push(Item::Placeholder(Placeholder::ScieBindingCmd(cmd)))
                    }
                    ["scie", "env", name] => items.push(Item::Placeholder(Placeholder::Env(name))),
                    ["scie", "lift"] => items.push(Item::Placeholder(Placeholder::ScieLift)),
                    _ => items.push(Item::Placeholder(Placeholder::FileName(symbol))),
                }
                previous_char = Some('}');
                inside_placeholder = false;
                start = index + 1;
            }
            c => previous_char = Some(c),
        }
    }
    if items.is_empty() || text.len() - start > 0 {
        items.push(Item::Text(&text[start..]))
    }

    Ok(Parsed { items })
}

#[cfg(test)]
mod tests {
    use super::{parse, Item, Placeholder};

    #[test]
    fn no_placeholders() {
        assert_eq!(vec![Item::Text("")], parse("").unwrap().items);
        assert_eq!(vec![Item::Text("b")], parse("b").unwrap().items);
        assert_eq!(vec![Item::Text("bob")], parse("bob").unwrap().items);
    }

    #[test]
    fn invalid_placeholder() {
        assert!(parse("{").is_err());
        assert!(parse("{}").is_err());
        assert!(parse("{placeholder.cannot.include.'{'}").is_err());
        assert_eq!(vec![Item::Text("}")], parse("}").unwrap().items);
    }

    #[test]
    fn scie() {
        assert_eq!(
            vec![Item::Placeholder(Placeholder::Scie)],
            parse("{scie}").unwrap().items
        );
        assert_eq!(
            vec![Item::Text("a"), Item::Placeholder(Placeholder::Scie)],
            parse("a{scie}").unwrap().items
        );
        assert_eq!(
            vec![
                Item::Text("a"),
                Item::Placeholder(Placeholder::Scie),
                Item::Text("boot")
            ],
            parse("a{scie}boot").unwrap().items
        );
    }

    #[test]
    fn scie_bindings() {
        assert_eq!(
            vec![Item::Placeholder(Placeholder::ScieBindings)],
            parse("{scie.bindings}").unwrap().items
        );
        assert_eq!(
            vec![
                Item::Text("A "),
                Item::Placeholder(Placeholder::ScieBindings)
            ],
            parse("A {scie.bindings}").unwrap().items
        );
        assert_eq!(
            vec![
                Item::Text("A "),
                Item::Placeholder(Placeholder::ScieBindings),
                Item::Text(" warmer")
            ],
            parse("A {scie.bindings} warmer").unwrap().items
        );
    }

    #[test]
    fn scie_bindings_cmd() {
        assert_eq!(
            vec![Item::Placeholder(Placeholder::ScieBindingCmd("do"))],
            parse("{scie.bindings.do}").unwrap().items
        );
        assert_eq!(
            vec![
                Item::Placeholder(Placeholder::ScieBindingCmd("dotted.cmd.name")),
                Item::Text("/venv/pex"),
            ],
            parse("{scie.bindings.dotted.cmd.name}/venv/pex")
                .unwrap()
                .items
        );
    }

    #[test]
    fn scie_env() {
        assert_eq!(
            vec![Item::Placeholder(Placeholder::Env("PATH"))],
            parse("{scie.env.PATH}").unwrap().items
        );
        assert_eq!(
            vec![Item::Placeholder(Placeholder::Env("dotted.env.var.name"))],
            parse("{scie.env.dotted.env.var.name}").unwrap().items
        );
    }

    #[test]
    fn file_name() {
        assert_eq!(
            vec![
                Item::Placeholder(Placeholder::FileName("python")),
                Item::Text("/bin/python")
            ],
            parse("{python}/bin/python").unwrap().items
        );
        assert_eq!(
            vec![Item::Placeholder(Placeholder::FileName("dotted.file.name"))],
            parse("{dotted.file.name}").unwrap().items
        );
    }

    #[test]
    fn escaping() {
        assert_eq!(
            vec![Item::LeftBrace, Item::Text("}")],
            parse("{{}").unwrap().items,
        );
        assert_eq!(
            vec![
                Item::Placeholder(Placeholder::FileName("node")),
                Item::Text("/a/path/with"),
                Item::LeftBrace,
                Item::Text("strange}characters/"),
                Item::LeftBrace,
                Item::Placeholder(Placeholder::Env("OPT")),
                Item::Text("}"),
            ],
            parse("{node}/a/path/with{{strange}characters/{{{scie.env.OPT}}")
                .unwrap()
                .items
        );
    }
}
