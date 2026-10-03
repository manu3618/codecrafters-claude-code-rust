use std::fmt;
use std::fs;
use std::path;
use std::str::FromStr;

#[derive(Debug)]
pub struct Skill {
    pub frontmatter: Frontmatter,
    body: String,
}

#[derive(Debug)]
pub struct SkillParseError;

impl fmt::Display for Skill {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "---\n{}\n---", self.frontmatter)?;
        writeln!(f, "{}", self.body)
    }
}

impl FromStr for Skill {
    type Err = SkillParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split("---");
        if let (Some(_), Some(frontmatter), Some(body), None) =
            (split.next(), split.next(), split.next(), split.next())
        {
            Ok(Self {
                frontmatter: Frontmatter::from_str(frontmatter)?,
                body: body.trim().into(),
            })
        } else {
            Err(SkillParseError)
        }
    }
}

impl Skill {
    pub fn get_body(&self, arguments: &str) -> String {
        let arg_list: Vec<&str> = arguments
            .split(' ')
            .filter(|s| !s.is_empty())
            .collect();
        let mut b = self.body.clone();
        for idx in (0..arg_list.len()).rev() {
            b = b.replace(format!("$ARGUMENT[{}]", idx).as_str(), arg_list[idx]);
            b = b.replace(format!("${}", idx).as_str(), arg_list[idx]);
        }
        b.replace("$ARGUMENTS", arguments)
    }
}

#[derive(Debug)]
pub struct Frontmatter {
    pub name: String,
    pub description: String,
}

impl fmt::Display for Frontmatter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "name: {}", self.name)?;
        writeln!(f, "description: {}", self.description)
    }
}

impl FromStr for Frontmatter {
    type Err = SkillParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut name = String::new();
        let mut description = String::new();
        for line in s.lines() {
            if line.is_empty() {
                continue;
            }
            let mut split = line.splitn(2, ':');
            if let (Some(key), Some(content), None) = (split.next(), split.next(), split.next()) {
                if key == "name" {
                    name = String::from(content.trim());
                } else if key == "description" {
                    description = String::from(content.trim());
                } else {
                    dbg!("unknown frontmatter key");
                    dbg!(key);
                    return Err(SkillParseError);
                }
            } else {
                dbg!("unparsable line");
                dbg!(&line);
                return Err(SkillParseError);
            }
        }
        Ok(Self { name, description })
    }
}

/// extract skills from folder
///
/// skills must be present in SKILL.md files, one skill per file.
///
/// the folder should look like
/// ```md
/// .claude/
/// └── skills/
///     ├── apple/
///     │   └── SKILL.md
///     └── grape/
///         └── SKILL.md
/// ```
pub fn get_skills(folder: &path::Path) -> Vec<Skill> {
    walk_dir(folder.to_path_buf())
        .iter()
        .filter_map(|p| {
            if p.file_name()? == path::Path::new("SKILL.md") {
                let mut s = Skill::from_str(fs::read_to_string(p).ok()?.as_str()).ok()?;
                s.frontmatter.name = p
                    .parent()
                    .unwrap()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .into();
                Some(s)
            } else {
                None
            }
        })
        .collect()
}

fn walk_dir(path: path::PathBuf) -> Vec<path::PathBuf> {
    if !path.is_dir() {
        return Vec::new();
    }
    let mut entries = Vec::new();
    entries.push(path.clone());
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_dir() {
            entries.append(&mut walk_dir(entry.path()))
        }
        entries.push(entry.path())
    }
    entries
}

mod test {
    use super::*;
    #[test]
    fn simple_skill() {
        let skill = r#"
---
name: apple
description: Deploys the apple service to production.
---

Run the deploy script and report the version that was deployed."#
            .trim();
        let skill = Skill::from_str(skill).unwrap();
        assert!(skill.frontmatter.name == "apple");
        assert!(skill.frontmatter.description == "Deploys the apple service to production.");
    }
}
