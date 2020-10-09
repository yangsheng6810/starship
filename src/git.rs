use once_cell::sync::OnceCell;
use std::fs;
use std::path::{Path, PathBuf};

use crate::utils;

#[derive(Default, Debug, PartialEq)]
pub struct GitStatus {
    pub untracked: usize,
    pub added: usize,
    pub modified: usize,
    pub renamed: usize,
    pub deleted: usize,
    pub stashed: usize,
    pub unmerged: usize,
    pub ahead: usize,
    pub behind: usize,
    pub diverged: usize,
    pub conflicted: usize,
    pub staged: usize,
}

#[derive(Debug)]
pub struct Repository {
    pub git_dir: PathBuf,
    pub root_dir: PathBuf,
    branch: OnceCell<String>,
    status: OnceCell<GitStatus>,
    hash: OnceCell<Option<String>>,
}

impl Repository {
    pub fn discover(path: &Path) -> Option<Self> {
        log::trace!("Checking for Git instance: {:?}", path);
        if let Some(repository) = Repository::scan(path) {
            return Some(repository);
        }

        match path.parent() {
            Some(parent) => Repository::discover(parent),
            None => None,
        }
    }

    fn scan(path: &Path) -> Option<Self> {
        let git_dir = path.join(".git");
        if !git_dir.exists() {
            return None;
        }

        log::trace!("Git repository found");
        Some(Repository {
            git_dir,
            root_dir: path.into(),
            branch: OnceCell::new(),
            status: OnceCell::new(),
            hash: OnceCell::new(),
        })
    }

    pub fn status(&self) -> &GitStatus {
        self.status.get_or_init(|| self.get_status())
    }

    fn get_status(&self) -> GitStatus {
        let output = match utils::exec_cmd(
            "git",
            &[
                "--git-dir",
                self.git_dir.to_str().unwrap(),
                "status",
                "--porcelain",
            ],
        ) {
            Some(output) => output.stdout,
            None => return Default::default(),
        };
        parse_porcelain_output(output)
    }

    pub fn branch(&self) -> &String {
        self.branch.get_or_init(|| match self.get_branch() {
            Some(branch) => branch,
            None => String::from("HEAD"),
        })
    }

    fn get_branch(&self) -> Option<String> {
        let head_file = self.git_dir.join("HEAD");
        let head_contents = fs::read_to_string(head_file).ok()?;
        let branch_start = head_contents.rfind('/')?;
        let branch_name = &head_contents[branch_start + 1..];
        let trimmed_branch_name = branch_name.trim_end();
        Some(trimmed_branch_name.into())
    }

    pub fn hash(&self) -> &Option<String> {
        self.hash.get_or_init(|| self.get_hash())
    }

    fn get_hash(&self) -> Option<String> {
        let output = utils::exec_cmd(
            "git",
            &[
                "--git-dir",
                self.git_dir.to_str().unwrap(),
                "rev-parse",
                "HEAD",
            ],
        )?;
        Some(output.stdout)
    }
}

/// Parse git status values from `git status --porcelain`
///
/// Example porcelain output:
/// ```code
///  M src/prompt.rs
///  M src/main.rs
/// ```
fn parse_porcelain_output<S: Into<String>>(porcelain: S) -> GitStatus {
    let porcelain_str = porcelain.into();
    let porcelain_lines = porcelain_str.lines();
    let mut vcs_status: GitStatus = Default::default();

    porcelain_lines.for_each(|line| {
        let mut characters = line.chars();

        // Extract the first two letter of each line
        let letter_codes = (
            characters.next().unwrap_or(' '),
            characters.next().unwrap_or(' '),
        );

        // TODO: Revisit conflict and staged logic
        if letter_codes.0 == letter_codes.1 {
            vcs_status.conflicted += 1
        } else {
            increment_git_status(&mut vcs_status, 'S');
            increment_git_status(&mut vcs_status, letter_codes.1);
        }
    });

    vcs_status
}

/// Update the cumulative git status, given the "short format" letter of a file's status
/// https://git-scm.com/docs/git-status#_short_format
fn increment_git_status(vcs_status: &mut GitStatus, letter: char) {
    match letter {
        'A' => vcs_status.added += 1,
        'M' => vcs_status.modified += 1,
        'D' => vcs_status.deleted += 1,
        'R' => vcs_status.renamed += 1,
        'C' => vcs_status.added += 1,
        'U' => vcs_status.modified += 1,
        '?' => vcs_status.untracked += 1,
        _ => (),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_parse_empty_porcelain_output() -> io::Result<()> {
        let output = parse_porcelain_output("");

        let expected: GitStatus = Default::default();
        assert_eq!(output, expected);
        Ok(())
    }

    #[test]
    fn test_parse_porcelain_output() -> io::Result<()> {
        let output = parse_porcelain_output(
            "M src/prompt.rs
MM src/main.rs
A src/formatter.rs
? README.md",
        );

        let expected = GitStatus {
            modified: 2,
            added: 1,
            untracked: 1,
            ..Default::default()
        };
        assert_eq!(output, expected);
        Ok(())
    }
}
