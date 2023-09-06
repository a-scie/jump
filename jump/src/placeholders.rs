// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

#[cfg_attr(test, derive(Eq, PartialEq))]
#[derive(Debug)]
pub(crate) struct ScieBindingEnv<'a> {
    pub(crate) binding: &'a str,
    pub(crate) env: &'a str,
}

#[cfg_attr(test, derive(Eq, PartialEq))]
#[derive(Debug)]
pub(crate) enum Placeholder<'a> {
    Env(&'a str),
    FileHash(&'a str),
    FileName(&'a str),
    UserCacheDir(&'a str),
    Scie,
    ScieBase,
    ScieBindings,
    ScieBindingCmd(&'a str),
    ScieBindingEnv(ScieBindingEnv<'a>),
    ScieLift,
    SciePlatform,
    SciePlatformArch,
    SciePlatformOs,
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
    let mut inside_placeholder = 0;
    let mut start = 0_usize;

    if "{" == text {
        return Err(
            "Encountered text of '{'. If a literal '{' is intended, escape it like so: '{{'."
                .to_string(),
        );
    }
    for (index, current_char) in text.chars().enumerate() {
        match current_char {
            '{' if inside_placeholder == 0 => {
                if index - start > 0 {
                    items.push(Item::Text(&text[start..index]))
                }
                previous_char = Some('{');
                inside_placeholder = 1;
                start = index + 1;
            }
            '{' if inside_placeholder > 0 && Some('{') == previous_char => {
                items.push(Item::LeftBrace);
                inside_placeholder = 0;
                start = index + 1;
            }
            '{' if inside_placeholder > 0 => {
                inside_placeholder += 1;
            }
            '}' if inside_placeholder > 1 => {
                inside_placeholder -= 1;
            }
            '}' if inside_placeholder == 1 => {
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
                    ["scie", "base"] => items.push(Item::Placeholder(Placeholder::ScieBase)),
                    ["scie", "bindings"] => {
                        items.push(Item::Placeholder(Placeholder::ScieBindings))
                    }
                    ["scie", "bindings", binding] => {
                        match binding.splitn(2, ':').collect::<Vec<_>>()[..] {
                            [name, env] => items.push(Item::Placeholder(
                                Placeholder::ScieBindingEnv(ScieBindingEnv { binding: name, env }),
                            )),
                            _ => {
                                items.push(Item::Placeholder(Placeholder::ScieBindingCmd(binding)))
                            }
                        }
                    }
                    ["scie", "env", env] => items.push(Item::Placeholder(Placeholder::Env(env))),
                    ["scie", "files", name] => {
                        items.push(Item::Placeholder(Placeholder::FileName(name)))
                    }
                    ["scie", "files:hash", name] => {
                        items.push(Item::Placeholder(Placeholder::FileHash(name)))
                    }
                    ["scie", "user", cache_dir] => {
                        match cache_dir.splitn(2, '=').collect::<Vec<_>>()[..] {
                            ["cache_dir", fallback] => {
                                items.push(Item::Placeholder(Placeholder::UserCacheDir(fallback)))
                            }
                            ["cache_dir"] => {
                                return Err(
                                    "The {{scie.user.cache_dir}} requires a fallback value; e.g.: \
                                    {{scie.user.cache_dir=~/.cache}}"
                                        .to_string(),
                                )
                            }
                            _ => {
                                return Err(format!(
                                    "Unrecognized placeholder in the {{scie.user.*}} \
                                    namespace: {{scie.user.{cache_dir}}}"
                                ))
                            }
                        }
                    }
                    ["scie", "lift"] => items.push(Item::Placeholder(Placeholder::ScieLift)),
                    ["scie", "platform"] => {
                        items.push(Item::Placeholder(Placeholder::SciePlatform))
                    }
                    ["scie", "platform", "arch"] => {
                        items.push(Item::Placeholder(Placeholder::SciePlatformArch))
                    }
                    ["scie", "platform", "os"] => {
                        items.push(Item::Placeholder(Placeholder::SciePlatformOs))
                    }
                    _ => items.push(Item::Placeholder(Placeholder::FileName(symbol))),
                }
                previous_char = Some('}');
                inside_placeholder = 0;
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
    use crate::placeholders::ScieBindingEnv;

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
    fn scie_base() {
        assert_eq!(
            vec![Item::Placeholder(Placeholder::ScieBase)],
            parse("{scie.base}").unwrap().items
        );
        assert_eq!(
            vec![
                Item::Placeholder(Placeholder::ScieBase),
                Item::Text("/here")
            ],
            parse("{scie.base}/here").unwrap().items
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
    fn scie_bindings_env() {
        assert_eq!(
            vec![Item::Placeholder(Placeholder::ScieBindingEnv(
                ScieBindingEnv {
                    binding: "do",
                    env: "FOO"
                }
            ))],
            parse("{scie.bindings.do:FOO}").unwrap().items
        );
        assert_eq!(
            vec![
                Item::Placeholder(Placeholder::ScieBindingEnv(ScieBindingEnv {
                    binding: "dotted.cmd.name",
                    env: "BAR"
                })),
                Item::Text("/venv/pex"),
            ],
            parse("{scie.bindings.dotted.cmd.name:BAR}/venv/pex")
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
        assert_eq!(
            vec![Item::Placeholder(Placeholder::Env("embedded={brackets}"))],
            parse("{scie.env.embedded={brackets}}").unwrap().items
        );
        assert_eq!(
            vec![Item::Placeholder(Placeholder::Env(
                "embedded={scie.env.doubly_embedded={brackets}}"
            ))],
            parse("{scie.env.embedded={scie.env.doubly_embedded={brackets}}}")
                .unwrap()
                .items
        );
    }

    #[test]
    fn file_hash() {
        assert_eq!(
            vec![Item::Placeholder(Placeholder::FileHash("python"))],
            parse("{scie.files:hash.python}").unwrap().items
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
            vec![
                Item::Placeholder(Placeholder::FileName("python")),
                Item::Text("/bin/python")
            ],
            parse("{scie.files.python}/bin/python").unwrap().items
        );
        assert_eq!(
            vec![
                Item::Placeholder(Placeholder::FileName("{scie.env.PYTHON}")),
                Item::Text("/bin/python")
            ],
            parse("{scie.files.{scie.env.PYTHON}}/bin/python")
                .unwrap()
                .items
        );
        assert_eq!(
            vec![Item::Placeholder(Placeholder::FileName("dotted.file.name"))],
            parse("{dotted.file.name}").unwrap().items
        );
    }

    #[test]
    fn user_cache_dir() {
        assert!(parse("{scie.user.config_dir}").is_err());
        assert!(parse("{scie.user.cache_dir}").is_err());
        assert_eq!(
            vec![Item::Placeholder(Placeholder::UserCacheDir("~/fall/back"))],
            parse("{scie.user.cache_dir=~/fall/back}").unwrap().items
        );
        assert_eq!(
            vec![Item::Placeholder(Placeholder::UserCacheDir(
                "{scie.env.CACHE_DIR=~/fall/back}"
            ))],
            parse("{scie.user.cache_dir={scie.env.CACHE_DIR=~/fall/back}}")
                .unwrap()
                .items
        );
    }

    #[test]
    fn platform() {
        assert_eq!(
            vec![Item::Placeholder(Placeholder::SciePlatform)],
            parse("{scie.platform}").unwrap().items,
        );
        assert_eq!(
            vec![Item::Placeholder(Placeholder::SciePlatformArch)],
            parse("{scie.platform.arch}").unwrap().items,
        );
        assert_eq!(
            vec![Item::Placeholder(Placeholder::SciePlatformOs)],
            parse("{scie.platform.os}").unwrap().items,
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
