// Copyright 2023 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::ffi::OsString;

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
            name: name.into(),
            default: None,
        }),
        [name, default] => Ok(ParsedEnv {
            name: name.into(),
            default: Some(default.into()),
        }),
        _ => Err("Expected {{scie.env.<name>}} <name> placeholder to be a non-empty string".into()),
    }
}

struct EnvParser<'a> {
    ambient_env: &'a IndexMap<OsString, OsString>,
    env: IndexMap<String, String>,
    key_stack: Vec<String>,
    parsed: IndexMap<String, String>,
}

impl<'a> EnvParser<'a> {
    fn new(
        env_vars: &IndexMap<EnvVar, Option<String>>,
        ambient_env: &'a IndexMap<OsString, OsString>,
    ) -> Self {
        let mut env = IndexMap::new();
        for (key, value) in env_vars.iter() {
            if let Some(val) = value {
                match key {
                    EnvVar::Default(name) => {
                        if ambient_env.get(&OsString::from(name)).is_none() {
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
            ambient_env,
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
                Item::Placeholder(Placeholder::UserCacheDir(fallback)) => {
                    reified.push_str(&format!("{{scie.user.cache_dir={fallback}}}"))
                }
                Item::Placeholder(Placeholder::Scie) => reified.push_str("{scie}"),
                Item::Placeholder(Placeholder::ScieArgv0) => reified.push_str("{scie.argv0}"),
                Item::Placeholder(Placeholder::ScieBase) => reified.push_str("{scie.base}"),
                Item::Placeholder(Placeholder::ScieBindings) => reified.push_str("{scie.bindings}"),
                Item::Placeholder(Placeholder::ScieBindingCmd(cmd)) => {
                    reified.push_str(&format!("{{scie.bindings.{cmd}}}"))
                }
                Item::Placeholder(Placeholder::ScieBindingEnv(ScieBindingEnv { binding, env })) => {
                    reified.push_str(&format!("{{scie.bindings.{binding}:{env}}}"))
                }
                Item::Placeholder(Placeholder::ScieJump) => reified.push_str("{scie.jump}"),
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
            if let Some(val) = self
                .ambient_env
                .get(&OsString::from(&parsed_env.name))
                .and_then(|v| v.to_owned().into_string().ok())
            {
                val
            } else {
                parsed_env.default.unwrap_or_default()
            }
        } else if let Some(val) = self
            .env
            .get(&parsed_env.name)
            .map(|v| v.to_owned())
            .or_else(|| {
                self.ambient_env
                    .get(&OsString::from(&parsed_env.name))
                    .and_then(|v| v.to_owned().into_string().ok())
            })
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
            self.key_stack.push(key.into());
            let parsed_value = self.reify_env(value)?;
            self.key_stack.pop();
            self.parsed.insert(key.into(), parsed_value);
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

pub(crate) fn prepare_env(
    cmd: &Cmd,
    ambient_env: &IndexMap<OsString, OsString>,
) -> Result<IndexMap<String, String>, String> {
    EnvParser::new(&cmd.env, ambient_env).parse_env()
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use crate::cmd_env::{EnvParser, ParsedEnv, parse_scie_env_placeholder};
    use crate::config::EnvVar;

    #[test]
    fn parse_env_placeholder() {
        assert_eq!(
            ParsedEnv {
                name: "FOO".into(),
                default: None
            },
            parse_scie_env_placeholder("FOO").unwrap()
        );
        assert_eq!(
            ParsedEnv {
                name: "FOO".into(),
                default: Some("bar".into())
            },
            parse_scie_env_placeholder("FOO=bar").unwrap()
        );
        assert_eq!(
            ParsedEnv {
                name: "FOO".into(),
                default: Some("bar=baz".into())
            },
            parse_scie_env_placeholder("FOO=bar=baz").unwrap()
        );
    }

    #[test]
    fn self_recurse() {
        let ambient_env = [("PATH".into(), "/test/path".into())].into();
        let env_parser = EnvParser::new(
            &[(
                EnvVar::Replace("PATH".into()),
                Some("foo:{scie.env.PATH}".into()),
            )]
            .into(),
            &ambient_env,
        );

        let expected: IndexMap<String, String> = [("PATH".into(), "foo:/test/path".into())].into();

        assert_eq!(expected, env_parser.parse_env().unwrap());
    }

    #[test]
    fn multi_step_recurse() {
        let ambient_env = [("PATH".into(), "/test/path".into())].into();
        let env_parser = EnvParser::new(
            &[
                (
                    EnvVar::Replace("PATH".into()),
                    Some("foo:{scie.env.X}".into()),
                ),
                (
                    EnvVar::Replace("X".into()),
                    Some("{scie.env.PATH}:bar".into()),
                ),
            ]
            .into(),
            &ambient_env,
        );

        let expected: IndexMap<String, String> = [
            ("PATH".into(), "foo:/test/path:bar".into()),
            ("X".into(), "/test/path:bar".into()),
        ]
        .into();

        assert_eq!(expected, env_parser.parse_env().unwrap());
    }

    #[test]
    fn test_dynamic_env_var_name() {
        let cmd_env: IndexMap<EnvVar, Option<String>> = [
            (
                EnvVar::Replace("__PYTHON_3_8".into()),
                Some("{cpython38}/python/bin/python3.8".into()),
            ),
            (
                EnvVar::Replace("__PYTHON_3_9".into()),
                Some("{cpython39}/python/bin/python3.9".into()),
            ),
            (
                EnvVar::Replace("__PYTHON".into()),
                Some("{scie.env.__PYTHON_3_{scie.env.__PYTHON_MINOR=9}}".into()),
            ),
        ]
        .into();

        let expected: IndexMap<String, String> = [
            (
                "__PYTHON_3_8".into(),
                "{scie.files.cpython38}/python/bin/python3.8".into(),
            ),
            (
                "__PYTHON_3_9".into(),
                "{scie.files.cpython39}/python/bin/python3.9".into(),
            ),
            (
                "__PYTHON".into(),
                "{scie.files.cpython39}/python/bin/python3.9".into(),
            ),
        ]
        .into();

        assert_eq!(
            expected,
            EnvParser::new(&cmd_env, &IndexMap::new())
                .parse_env()
                .unwrap()
        );

        let expected: IndexMap<String, String> = [
            (
                "__PYTHON_3_8".into(),
                "{scie.files.cpython38}/python/bin/python3.8".into(),
            ),
            (
                "__PYTHON_3_9".into(),
                "{scie.files.cpython39}/python/bin/python3.9".into(),
            ),
            (
                "__PYTHON".into(),
                "{scie.files.cpython38}/python/bin/python3.8".into(),
            ),
        ]
        .into();

        assert_eq!(
            expected,
            EnvParser::new(&cmd_env, &[("__PYTHON_MINOR".into(), "8".into())].into())
                .parse_env()
                .unwrap()
        );
    }

    #[test]
    fn test_dynamic_env_var_default() {
        let cmd_env: IndexMap<EnvVar, Option<String>> = [(
            EnvVar::Replace("FOO".into()),
            Some("{scie.env.BAR={scie.env.BAZ=spam}}".into()),
        )]
        .into();

        let expected: IndexMap<String, String> = [("FOO".into(), "spam".into())].into();

        assert_eq!(
            expected,
            EnvParser::new(&cmd_env, &IndexMap::new())
                .parse_env()
                .unwrap()
        );

        let expected: IndexMap<String, String> = [("FOO".to_string(), "eggs".to_string())].into();

        assert_eq!(
            expected,
            EnvParser::new(&cmd_env, &[("BAZ".into(), "eggs".into())].into())
                .parse_env()
                .unwrap()
        );

        let expected: IndexMap<String, String> = [("FOO".to_string(), "cheese".to_string())].into();

        assert_eq!(
            expected,
            EnvParser::new(&cmd_env, &[("BAR".into(), "cheese".into())].into())
                .parse_env()
                .unwrap()
        );
    }

    #[test]
    fn test_excessively_dynamic_env() {
        let cmd_env = [(
            EnvVar::Replace("A".into()),
            Some("PreA{scie.env.PreB{scie.env.B}PostB=PreC{scie.env.C=c}PostC}PostA".into()),
        )]
        .into();

        let expected: IndexMap<String, String> =
            [("A".into(), "PreAPreCcPostCPostA".into())].into();

        assert_eq!(
            expected,
            EnvParser::new(&cmd_env, &IndexMap::new())
                .parse_env()
                .unwrap()
        );

        let expected: IndexMap<String, String> =
            [("A".into(), "PreAPreC_Cee_PostCPostA".into())].into();

        assert_eq!(
            expected,
            EnvParser::new(&cmd_env, &[("C".into(), "_Cee_".into())].into())
                .parse_env()
                .unwrap()
        );

        let expected: IndexMap<String, String> = [("A".into(), "PreA_Bee_PostA".into())].into();

        assert_eq!(
            expected,
            EnvParser::new(&cmd_env, &[("PreBPostB".into(), "_Bee_".into())].into())
                .parse_env()
                .unwrap()
        );

        let expected: IndexMap<String, String> = [("A".into(), "PreA_Buzz_PostA".into())].into();

        assert_eq!(
            expected,
            EnvParser::new(
                &cmd_env,
                &[
                    ("B".into(), "_Bee_".into()),
                    ("PreB_Bee_PostB".into(), "_Buzz_".into())
                ]
                .into()
            )
            .parse_env()
            .unwrap()
        );
    }

    #[test]
    fn test_ignored_placeholders() {
        let ambient_env = [("PATH".into(), "/test/path".into())].into();
        let env_parser = EnvParser::new(
            &[(
                EnvVar::Replace("PATH".into()),
                Some("{foo}:{scie.env.PATH}:{scie}:{scie.base}:{scie.files.bar}:baz{{}".into()),
            )]
            .into(),
            &ambient_env,
        );

        let expected: IndexMap<String, String> = [(
            "PATH".into(),
            "{scie.files.foo}:/test/path:{scie}:{scie.base}:{scie.files.bar}:baz{}".into(),
        )]
        .into();

        assert_eq!(expected, env_parser.parse_env().unwrap());
    }

    #[test]
    fn test_user_cache_dir_placeholder() {
        let ambient_env = IndexMap::new();
        let env_parser = EnvParser::new(
            &[(
                EnvVar::Replace("SCIE_BASE".into()),
                Some("{scie.user.cache_dir=foo}".into()),
            )]
            .into(),
            &ambient_env,
        );

        let expected: IndexMap<String, String> =
            [("SCIE_BASE".into(), "{scie.user.cache_dir=foo}".into())].into();

        assert_eq!(expected, env_parser.parse_env().unwrap());
    }
}
