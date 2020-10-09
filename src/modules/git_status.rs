use super::{Context, Module, RootModuleConfig};

use crate::configs::git_status::GitStatusConfig;
use crate::formatter::StringFormatter;
use crate::segment::Segment;

const ALL_STATUS_FORMAT: &str = "$conflicted$stashed$deleted$renamed$modified$staged$untracked";

/// Creates a module with the Git branch in the current directory
///
/// Will display the branch name if the current directory is a git repo
/// By default, the following symbols will be used to represent the repo's status:
///   - `=` – This branch has merge conflicts
///   - `⇡` – This branch is ahead of the branch being tracked
///   - `⇣` – This branch is behind of the branch being tracked
///   - `⇕` – This branch has diverged from the branch being tracked
///   - `?` — There are untracked files in the working directory
///   - `$` — A stash exists for the local repository
///   - `!` — There are file modifications in the working directory
///   - `+` — A new file has been added to the staging area
///   - `»` — A renamed file has been added to the staging area
///   - `✘` — A file's deletion has been added to the staging area
pub fn module<'a>(context: &'a Context) -> Option<Module<'a>> {
    let repo = context.repo().as_ref()?;
    let status = repo.status();

    let mut module = context.new_module("git_status");
    let config: GitStatusConfig = GitStatusConfig::try_load(module.config);

    let parsed = StringFormatter::new(config.format).and_then(|formatter| {
        formatter
            .map_meta(|variable, _| match variable {
                "all_status" => Some(ALL_STATUS_FORMAT),
                _ => None,
            })
            .map_style(|variable: &str| match variable {
                "style" => Some(Ok(config.style)),
                _ => None,
            })
            .map_variables_to_segments(|variable: &str| {
                let segments = match variable {
                    "stashed" => format_count(config.stashed, "git_status.stashed", status.stashed),
                    "ahead" => format_count(config.ahead, "git_status.ahead", status.ahead),
                    "behind" => format_count(config.behind, "git_status.behind", status.ahead),
                    "conflicted" => format_count(
                        config.conflicted,
                        "git_status.conflicted",
                        status.conflicted,
                    ),
                    "deleted" => format_count(config.deleted, "git_status.deleted", status.deleted),
                    "renamed" => format_count(config.renamed, "git_status.renamed", status.renamed),
                    "modified" => {
                        format_count(config.modified, "git_status.modified", status.modified)
                    }
                    "staged" => format_count(config.staged, "git_status.staged", status.staged),
                    "untracked" => {
                        format_count(config.untracked, "git_status.untracked", status.untracked)
                    }
                    _ => None,
                };
                segments.map(Ok)
            })
            .parse(None)
    });

    module.set_segments(match parsed {
        Ok(segments) => {
            if segments.is_empty() {
                return None;
            } else {
                segments
            }
        }
        Err(error) => {
            log::warn!("Error in module `git_status`:\n{}", error);
            return None;
        }
    });

    Some(module)
}

fn format_text<F>(format_str: &str, config_path: &str, mapper: F) -> Option<Vec<Segment>>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if let Ok(formatter) = StringFormatter::new(format_str) {
        formatter
            .map(|variable| mapper(variable).map(Ok))
            .parse(None)
            .ok()
    } else {
        log::warn!("Error parsing format string `{}`", &config_path);
        None
    }
}

fn format_count(format_str: &str, config_path: &str, count: usize) -> Option<Vec<Segment>> {
    if count == 0 {
        return None;
    }

    format_text(format_str, config_path, |variable| match variable {
        "count" => Some(count.to_string()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use ansi_term::{ANSIStrings, Color};
    use std::fs::{self, File};
    use std::io;
    use std::path::Path;
    use std::process::Command;

    use crate::test::{fixture_repo, FixtureProvider, ModuleRenderer};

    /// Right after the calls to git the filesystem state may not have finished
    /// updating yet causing some of the tests to fail. These barriers are placed
    /// after each call to git.
    /// This barrier is windows-specific though other operating systems may need it
    /// in the future.
    #[cfg(not(windows))]
    fn barrier() {}
    #[cfg(windows)]
    fn barrier() {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    fn format_output(symbols: &str) -> Option<String> {
        Some(format!(
            "{} ",
            Color::Red.bold().paint(format!("[{}]", symbols))
        ))
    }

    #[test]
    fn show_nothing_on_empty_dir() -> io::Result<()> {
        let repo_dir = tempfile::tempdir()?;

        let actual = ModuleRenderer::new("git_status")
            .path(repo_dir.path())
            .collect();
        let expected = None;

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_behind() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        behind(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .path(repo_dir.path())
            .collect();
        let expected = format_output("⇣");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_behind_with_count() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        behind(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .config(toml::toml! {
                [git_status]
                behind = "⇣$count"
            })
            .path(repo_dir.path())
            .collect();
        let expected = format_output("⇣1");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_ahead() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        File::create(repo_dir.path().join("readme.md"))?.sync_all()?;
        ahead(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("⇡");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_ahead_with_count() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        File::create(repo_dir.path().join("readme.md"))?.sync_all()?;
        ahead(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .config(toml::toml! {
                [git_status]
                ahead="⇡$count"
            })
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("⇡1");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_diverged() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        diverge(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("⇕");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_diverged_with_count() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        diverge(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .config(toml::toml! {
                [git_status]
                diverged=r"⇕⇡$ahead_count⇣$behind_count"
            })
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("⇕⇡1⇣1");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_conflicted() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        create_conflict(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("=");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_conflicted_with_count() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        create_conflict(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .config(toml::toml! {
                [git_status]
                conflicted = "=$count"
            })
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("=1");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_untracked_file() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        create_untracked(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("?");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_untracked_file_with_count() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        create_untracked(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .config(toml::toml! {
                [git_status]
                untracked = "?$count"
            })
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("?1");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn doesnt_show_untracked_file_if_disabled() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        create_untracked(&repo_dir.path())?;

        Command::new("git")
            .args(&["config", "status.showUntrackedFiles", "no"])
            .current_dir(repo_dir.path())
            .output()?;
        barrier();

        let actual = ModuleRenderer::new("git_status")
            .path(&repo_dir.path())
            .collect();
        let expected = None;

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_stashed() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;
        barrier();

        create_stash(&repo_dir.path())?;

        Command::new("git")
            .args(&["reset", "--hard", "HEAD"])
            .current_dir(repo_dir.path())
            .output()?;
        barrier();

        let actual = ModuleRenderer::new("git_status")
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("$");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_stashed_with_count() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;
        barrier();

        create_stash(&repo_dir.path())?;
        barrier();

        Command::new("git")
            .args(&["reset", "--hard", "HEAD"])
            .current_dir(repo_dir.path())
            .output()?;
        barrier();

        let actual = ModuleRenderer::new("git_status")
            .config(toml::toml! {
                [git_status]
                stashed = r"\$$count"
            })
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("$1");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_modified() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        create_modified(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("!");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_modified_with_count() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        create_modified(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .config(toml::toml! {
                [git_status]
                modified = "!$count"
            })
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("!1");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_staged_file() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        create_staged(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("+");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_staged_file_with_count() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        create_staged(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .config(toml::toml! {
                [git_status]
                staged = "+[$count](green)"
            })
            .path(&repo_dir.path())
            .collect();
        let expected = Some(format!(
            "{} ",
            ANSIStrings(&[
                Color::Red.bold().paint("[+"),
                Color::Green.paint("1"),
                Color::Red.bold().paint("]"),
            ])
        ));

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_renamed_file() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        create_renamed(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("»");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_renamed_file_with_count() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        create_renamed(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .config(toml::toml! {
                [git_status]
                renamed = "»$count"
            })
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("»1");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_deleted_file() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        create_deleted(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("✘");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    #[test]
    fn shows_deleted_file_with_count() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;

        create_deleted(&repo_dir.path())?;

        let actual = ModuleRenderer::new("git_status")
            .config(toml::toml! {
                [git_status]
                deleted = "✘$count"
            })
            .path(&repo_dir.path())
            .collect();
        let expected = format_output("✘1");

        assert_eq!(expected, actual);
        repo_dir.close()
    }

    // Whenever a file is manually renamed, git itself ('git status') does not treat such file as renamed,
    // but as untracked instead. The following test checks if manually deleted and manually renamed
    // files are tracked by git_status module in the same way 'git status' does.
    #[test]
    #[ignore]
    fn ignore_manually_renamed() -> io::Result<()> {
        let repo_dir = fixture_repo(FixtureProvider::GIT)?;
        File::create(repo_dir.path().join("a"))?.sync_all()?;
        File::create(repo_dir.path().join("b"))?.sync_all()?;
        Command::new("git")
            .args(&["add", "--all"])
            .current_dir(&repo_dir.path())
            .output()?;
        Command::new("git")
            .args(&["commit", "-m", "add new files", "--no-gpg-sign"])
            .current_dir(&repo_dir.path())
            .output()?;

        fs::remove_file(repo_dir.path().join("a"))?;
        fs::rename(repo_dir.path().join("b"), repo_dir.path().join("c"))?;
        barrier();

        let actual = ModuleRenderer::new("git_status")
            .path(&repo_dir.path())
            .config(toml::toml! {
                [git_status]
                ahead = "A"
                deleted = "D"
                untracked = "U"
                renamed = "R"
            })
            .collect();
        let expected = format_output("DUA");

        assert_eq!(actual, expected);

        repo_dir.close()
    }

    fn ahead(repo_dir: &Path) -> io::Result<()> {
        File::create(repo_dir.join("readme.md"))?.sync_all()?;

        Command::new("git")
            .args(&["commit", "-am", "Update readme", "--no-gpg-sign"])
            .current_dir(&repo_dir)
            .output()?;
        barrier();

        Ok(())
    }

    fn behind(repo_dir: &Path) -> io::Result<()> {
        Command::new("git")
            .args(&["reset", "--hard", "HEAD^"])
            .current_dir(repo_dir)
            .output()?;
        barrier();

        Ok(())
    }

    fn diverge(repo_dir: &Path) -> io::Result<()> {
        Command::new("git")
            .args(&["reset", "--hard", "HEAD^"])
            .current_dir(repo_dir)
            .output()?;
        barrier();

        fs::write(repo_dir.join("Cargo.toml"), " ")?;

        Command::new("git")
            .args(&["commit", "-am", "Update readme", "--no-gpg-sign"])
            .current_dir(repo_dir)
            .output()?;
        barrier();

        Ok(())
    }

    fn create_conflict(repo_dir: &Path) -> io::Result<()> {
        Command::new("git")
            .args(&["reset", "--hard", "HEAD^"])
            .current_dir(repo_dir)
            .output()?;
        barrier();

        fs::write(repo_dir.join("readme.md"), "# goodbye")?;

        Command::new("git")
            .args(&["add", "."])
            .current_dir(repo_dir)
            .output()?;
        barrier();

        Command::new("git")
            .args(&["commit", "-m", "Change readme", "--no-gpg-sign"])
            .current_dir(repo_dir)
            .output()?;
        barrier();

        Command::new("git")
            .args(&["pull", "--rebase"])
            .current_dir(repo_dir)
            .output()?;
        barrier();

        Ok(())
    }

    fn create_stash(repo_dir: &Path) -> io::Result<()> {
        File::create(repo_dir.join("readme.md"))?.sync_all()?;
        barrier();

        Command::new("git")
            .args(&["stash", "--all"])
            .current_dir(repo_dir)
            .output()?;
        barrier();

        Ok(())
    }

    fn create_untracked(repo_dir: &Path) -> io::Result<()> {
        File::create(repo_dir.join("license"))?.sync_all()?;

        Ok(())
    }

    fn create_modified(repo_dir: &Path) -> io::Result<()> {
        File::create(repo_dir.join("readme.md"))?.sync_all()?;

        Ok(())
    }

    fn create_staged(repo_dir: &Path) -> io::Result<()> {
        File::create(repo_dir.join("license"))?.sync_all()?;

        Command::new("git")
            .args(&["add", "."])
            .current_dir(repo_dir)
            .output()?;
        barrier();

        Ok(())
    }

    fn create_renamed(repo_dir: &Path) -> io::Result<()> {
        Command::new("git")
            .args(&["mv", "readme.md", "readme.md.bak"])
            .current_dir(repo_dir)
            .output()?;
        barrier();

        Command::new("git")
            .args(&["add", "-A"])
            .current_dir(repo_dir)
            .output()?;
        barrier();

        Ok(())
    }

    fn create_deleted(repo_dir: &Path) -> io::Result<()> {
        fs::remove_file(repo_dir.join("readme.md"))?;

        Ok(())
    }
}
