// Copyright 2023 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::env;

use indexmap::IndexMap;

use crate::config::{Cmd, EnvVar};
use crate::placeholders;
use crate::placeholders::{Item, Placeholder, ScieBindingEnv};

#[derive(Debug)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub(crate) struct ParsedEnv {
    pub(crate) name: String,
    pub(crate) default: Option<String>,
}

pub(crate) fn parse_scie_env_placeholder(env_var: &str) -> Result<ParsedEnv, String> {
    match env_var.splitn(2, '=').collect::<Vec<_>>()[..] {
        [name] => Ok(ParsedEnv {
            name: name.to_string(),
            default: None,
        }),
        [name, default] => Ok(ParsedEnv {
            name: name.to_string(),
            default: Some(default.to_string()),
        }),
        _ => Err(
            "Expected {{scie.env.<name>}} <name> placeholder to be a non-empty string".to_string(),
        ),
    }
}

struct EnvParser {
    env: IndexMap<String, String>,
    key_stack: Vec<String>,
    parsed: IndexMap<String, String>,
}

impl EnvParser {
    fn new(env_vars: &IndexMap<EnvVar, Option<String>>) -> Self {
        let mut env = IndexMap::new();
        for (key, value) in env_vars.iter() {
            if let Some(val) = value {
                match key {
                    EnvVar::Default(name) => {
                        if env::var_os(name).is_none() {
                            env.insert(name.to_owned(), val.to_owned());
                        }
                    }
                    EnvVar::Replace(name) => {
                        env.insert(name.to_owned(), val.to_owned());
                    }
                }
            }
        }
        Self {
            env,
            key_stack: vec![],
            parsed: IndexMap::new(),
        }
    }

    fn reify_env(&mut self, value: &str) -> Result<String, String> {
        let mut reified = String::with_capacity(value.len());
        let parsed = placeholders::parse(value)?;
        for item in &parsed.items {
            match item {
                Item::LeftBrace => reified.push('{'),
                Item::Text(text) => reified.push_str(text),
                Item::Placeholder(Placeholder::Env(name)) => {
                    reified.push_str(&self.reify_env_var(name)?)
                }
                Item::Placeholder(Placeholder::FileHash(name)) => {
                    reified.push_str(&format!("{{scie.files:hash.{name}}}"))
                }
                Item::Placeholder(Placeholder::FileName(name)) => {
                    reified.push_str(&format!("{{scie.files.{name}}}"))
                }
                Item::Placeholder(Placeholder::Scie) => reified.push_str("{scie}"),
                Item::Placeholder(Placeholder::ScieBase) => reified.push_str("{scie.base}"),
                Item::Placeholder(Placeholder::ScieBindings) => reified.push_str("{scie.bindings}"),
                Item::Placeholder(Placeholder::ScieBindingCmd(cmd)) => {
                    reified.push_str(&format!("{{scie.bindings.{cmd}}}"))
                }
                Item::Placeholder(Placeholder::ScieBindingEnv(ScieBindingEnv { binding, env })) => {
                    reified.push_str(&format!("{{scie.bindings.{binding}:{env}}}"))
                }
                Item::Placeholder(Placeholder::ScieLift) => reified.push_str("{scie.lift}"),
                Item::Placeholder(Placeholder::SciePlatform) => reified.push_str("{scie.platform}"),
                Item::Placeholder(Placeholder::SciePlatformArch) => {
                    reified.push_str("{scie.platform.arch}")
                }
                Item::Placeholder(Placeholder::SciePlatformOs) => {
                    reified.push_str("{scie.platform.os}")
                }
            }
        }
        Ok(reified)
    }

    fn reify_env_var(&mut self, name: &str) -> Result<String, String> {
        let reified_env_name = self.reify_env(name)?;
        let parsed_env = parse_scie_env_placeholder(&reified_env_name)?;
        let env_val = if self.key_stack.contains(&parsed_env.name) {
            // If we're already calculating a Cmd env var value for `key`, we can only
            // pull references to that `key` needed to compute the value from the
            // ambient environment.
            if let Ok(val) = env::var(&parsed_env.name) {
                val
            } else {
                parsed_env.default.unwrap_or_default()
            }
        } else if let Some(val) = self
            .env
            .get(&parsed_env.name)
            .map(|v| v.to_owned())
            .or_else(|| env::var(&parsed_env.name).ok())
        {
            self.parse_env_var(&parsed_env.name, &val)?
        } else {
            parsed_env.default.unwrap_or_default()
        };
        self.reify_env(&env_val)
    }

    fn parse_env_var(&mut self, key: &str, value: &str) -> Result<String, String> {
        self.parse_entry(key, value)?;
        self.parsed
            .get(key)
            .ok_or_else(|| format!("We just parsed an entry for {key}={value} above without error"))
            .map(String::to_owned)
    }

    fn parse_entry(&mut self, key: &str, value: &str) -> Result<(), String> {
        if !self.parsed.contains_key(key) {
            self.key_stack.push(key.to_string());
            let parsed_value = self.reify_env(value)?;
            self.key_stack.pop();
            self.parsed.insert(key.to_string(), parsed_value);
        }
        Ok(())
    }

    fn parse_env(mut self) -> Result<IndexMap<String, String>, String> {
        for (key, value) in self.env.clone() {
            self.parse_entry(&key, &value)?;
        }
        self.parsed.retain(|k, _| self.env.contains_key(k));
        Ok(self.parsed)
    }
}

pub(crate) fn prepare_env(cmd: &Cmd) -> Result<IndexMap<String, String>, String> {
    EnvParser::new(&cmd.env).parse_env()
}

#[cfg(test)]
mod tests {
    use std::env;

    use indexmap::IndexMap;

    use crate::cmd_env::{parse_scie_env_placeholder, EnvParser, ParsedEnv};
    use crate::config::EnvVar;

    #[test]
    fn parse_env_placeholder() {
        assert_eq!(
            ParsedEnv {
                name: "FOO".to_string(),
                default: None
            },
            parse_scie_env_placeholder("FOO").unwrap()
        );
        assert_eq!(
            ParsedEnv {
                name: "FOO".to_string(),
                default: Some("bar".to_string())
            },
            parse_scie_env_placeholder("FOO=bar").unwrap()
        );
        assert_eq!(
            ParsedEnv {
                name: "FOO".to_string(),
                default: Some("bar=baz".to_string())
            },
            parse_scie_env_placeholder("FOO=bar=baz").unwrap()
        );
    }

    #[test]
    fn self_recurse() {
        let env_parser = EnvParser::new(
            &[(
                EnvVar::Replace("PATH".to_string()),
                Some("foo:{scie.env.PATH}".to_string()),
            )]
            .into_iter()
            .collect::<IndexMap<_, _>>(),
        );

        let expected = [(
            "PATH".to_string(),
            format!("foo:{path}", path = env::var("PATH").unwrap()),
        )]
        .into_iter()
        .collect::<IndexMap<_, _>>();

        assert_eq!(expected, env_parser.parse_env().unwrap());
    }

    #[test]
    fn multi_step_recurse() {
        let env_parser = EnvParser::new(
            &[
                (
                    EnvVar::Replace("PATH".to_string()),
                    Some("foo:{scie.env.X}".to_string()),
                ),
                (
                    EnvVar::Replace("X".to_string()),
                    Some("{scie.env.PATH}:bar".to_string()),
                ),
            ]
            .into_iter()
            .collect::<IndexMap<_, _>>(),
        );

        let expected_path = env::var("PATH").unwrap();
        let expected = [
            ("PATH".to_string(), format!("foo:{expected_path}:bar")),
            ("X".to_string(), format!("{expected_path}:bar")),
        ]
        .into_iter()
        .collect::<IndexMap<_, _>>();

        assert_eq!(expected, env_parser.parse_env().unwrap());
    }

    #[test]
    fn test_dynamic_env_var_name() {
        let cmd_env = &[
            (
                EnvVar::Replace("__PYTHON_3_8".to_string()),
                Some("{cpython38}/python/bin/python3.8".to_string()),
            ),
            (
                EnvVar::Replace("__PYTHON_3_9".to_string()),
                Some("{cpython39}/python/bin/python3.9".to_string()),
            ),
            (
                EnvVar::Replace("__PYTHON".to_string()),
                Some("{scie.env.__PYTHON_3_{scie.env.__PYTHON_MINOR=9}}".to_string()),
            ),
        ]
        .into_iter()
        .collect::<IndexMap<_, _>>();

        let expected = [
            (
                "__PYTHON_3_8".to_string(),
                "{scie.files.cpython38}/python/bin/python3.8".to_string(),
            ),
            (
                "__PYTHON_3_9".to_string(),
                "{scie.files.cpython39}/python/bin/python3.9".to_string(),
            ),
            (
                "__PYTHON".to_string(),
                "{scie.files.cpython39}/python/bin/python3.9".to_string(),
            ),
        ]
        .into_iter()
        .collect::<IndexMap<_, _>>();

        assert_eq!(expected, EnvParser::new(cmd_env).parse_env().unwrap());

        env::set_var("__PYTHON_MINOR", "8");
        let expected = [
            (
                "__PYTHON_3_8".to_string(),
                "{scie.files.cpython38}/python/bin/python3.8".to_string(),
            ),
            (
                "__PYTHON_3_9".to_string(),
                "{scie.files.cpython39}/python/bin/python3.9".to_string(),
            ),
            (
                "__PYTHON".to_string(),
                "{scie.files.cpython38}/python/bin/python3.8".to_string(),
            ),
        ]
        .into_iter()
        .collect::<IndexMap<_, _>>();

        assert_eq!(expected, EnvParser::new(cmd_env).parse_env().unwrap());
    }

    #[test]
    fn test_dynamic_env_var_default() {
        let cmd_env = [(
            EnvVar::Replace("FOO".to_string()),
            Some("{scie.env.BAR={scie.env.BAZ=spam}}".to_string()),
        )]
        .into_iter()
        .collect::<IndexMap<_, _>>();

        let expected = [("FOO".to_string(), "spam".to_string())]
            .into_iter()
            .collect::<IndexMap<_, _>>();

        assert_eq!(expected, EnvParser::new(&cmd_env).parse_env().unwrap());

        env::set_var("BAZ", "eggs");
        let expected = [("FOO".to_string(), "eggs".to_string())]
            .into_iter()
            .collect::<IndexMap<_, _>>();

        assert_eq!(expected, EnvParser::new(&cmd_env).parse_env().unwrap());
        env::remove_var("BAZ");

        env::set_var("BAR", "cheese");
        let expected = [("FOO".to_string(), "cheese".to_string())]
            .into_iter()
            .collect::<IndexMap<_, _>>();

        assert_eq!(expected, EnvParser::new(&cmd_env).parse_env().unwrap());
        env::remove_var("BAR");
    }

    #[test]
    fn test_excessively_dynamic_env() {
        let cmd_env = [(
            EnvVar::Replace("A".to_string()),
            Some("PreA{scie.env.PreB{scie.env.B}PostB=PreC{scie.env.C=c}PostC}PostA".to_string()),
        )]
        .into_iter()
        .collect::<IndexMap<_, _>>();

        let expected = [("A".to_string(), "PreAPreCcPostCPostA".to_string())]
            .into_iter()
            .collect::<IndexMap<_, _>>();

        assert_eq!(expected, EnvParser::new(&cmd_env).parse_env().unwrap());

        env::set_var("C", "_Cee_");
        let expected = [("A".to_string(), "PreAPreC_Cee_PostCPostA".to_string())]
            .into_iter()
            .collect::<IndexMap<_, _>>();

        assert_eq!(expected, EnvParser::new(&cmd_env).parse_env().unwrap());
        env::remove_var("C");

        env::set_var("PreBPostB", "_Bee_");
        let expected = [("A".to_string(), "PreA_Bee_PostA".to_string())]
            .into_iter()
            .collect::<IndexMap<_, _>>();

        assert_eq!(expected, EnvParser::new(&cmd_env).parse_env().unwrap());
        env::remove_var("PreBPostB");

        env::set_var("B", "_Bee_");
        env::set_var("PreB_Bee_PostB", "_Buzz_");
        let expected = [("A".to_string(), "PreA_Buzz_PostA".to_string())]
            .into_iter()
            .collect::<IndexMap<_, _>>();

        assert_eq!(expected, EnvParser::new(&cmd_env).parse_env().unwrap());
        env::remove_var("B");
        env::remove_var("PreB_Bee_PostB");
    }

    #[test]
    fn test_ignored_placeholders() {
        let env_parser = EnvParser::new(
            &[(
                EnvVar::Replace("PATH".to_string()),
                Some(
                    "{foo}:{scie.env.PATH}:{scie}:{scie.base}:{scie.files.bar}:baz{{}".to_string(),
                ),
            )]
            .into_iter()
            .collect::<IndexMap<_, _>>(),
        );

        let expected = [(
            "PATH".to_string(),
            format!(
                "{{scie.files.foo}}:{path}:{{scie}}:{{scie.base}}:{{scie.files.bar}}:baz{{}}",
                path = env::var("PATH").unwrap()
            ),
        )]
        .into_iter()
        .collect::<IndexMap<_, _>>();

        assert_eq!(expected, env_parser.parse_env().unwrap());
    }
}
