//! Conservative desktop-entry matching: executable identity, never display-name guessing.
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DesktopIcon {
    pub icon: String,
    pub desktop_file: PathBuf,
}

pub(super) fn insert(
    index: &mut HashMap<PathBuf, Option<DesktopIcon>>,
    executable: PathBuf,
    candidate: DesktopIcon,
) {
    index
        .entry(executable)
        .and_modify(|existing| {
            if existing
                .as_ref()
                .is_some_and(|old| old.icon != candidate.icon)
            {
                // Multiple launchers can run one interpreter or control-center binary.
                // A missing icon is preferable to displaying another application's identity.
                *existing = None;
            }
        })
        .or_insert(Some(candidate));
}

pub(super) struct ExecutableCommand<'a> {
    pub program: &'a str,
    has_arguments: bool,
}

impl ExecutableCommand<'_> {
    pub fn identifies(&self, executable: &std::path::Path) -> bool {
        // Arguments select one script or sandboxed app; its runtime's executable
        // alone cannot identify other processes using that same runtime.
        executable
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| !self.has_arguments || !shared_launcher(name))
    }
}

pub(super) fn command_executable(arguments: &[String]) -> Option<ExecutableCommand<'_>> {
    let first = arguments.first()?.as_str();
    let index = if std::path::Path::new(first).file_name()? == "env" {
        arguments
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, argument)| argument.as_str() != "--" && !argument.contains('='))?
            .0
    } else {
        0
    };
    let program = arguments[index].as_str();
    if program.starts_with('-') {
        return None;
    }
    let command = ExecutableCommand {
        program,
        has_arguments: arguments.len() > index + 1,
    };
    command
        .identifies(std::path::Path::new(program))
        .then_some(command)
}

fn shared_launcher(name: &str) -> bool {
    name.starts_with("python")
        || name == "electron"
        || name.strip_prefix("electron").is_some_and(|version| {
            version.starts_with(|character: char| character.is_ascii_digit())
        })
        || matches!(
            name,
            "sh" | "bash"
                | "dash"
                | "zsh"
                | "fish"
                | "node"
                | "nodejs"
                | "perl"
                | "ruby"
                | "java"
                | "mono"
                | "dotnet"
                | "php"
                | "lua"
                | "tclsh"
                | "wish"
                | "wine"
                | "wine64"
                | "flatpak"
                | "snap"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(arguments: &[String]) -> Option<&str> {
        command_executable(arguments).map(|command| command.program)
    }

    fn icon(name: &str) -> DesktopIcon {
        DesktopIcon {
            icon: name.into(),
            desktop_file: "/fixture.desktop".into(),
        }
    }

    #[test]
    fn conflicting_launchers_do_not_borrow_an_unrelated_icon() {
        let mut index = HashMap::new();
        insert(&mut index, "/usr/bin/python3".into(), icon("first"));
        insert(&mut index, "/usr/bin/python3".into(), icon("second"));
        insert(&mut index, "/usr/bin/python3".into(), icon("first"));
        assert_eq!(index[&PathBuf::from("/usr/bin/python3")], None);
    }

    #[test]
    fn multiple_launchers_with_the_same_artwork_are_unambiguous() {
        let mut index = HashMap::new();
        insert(&mut index, "/usr/bin/editor".into(), icon("editor"));
        insert(&mut index, "/usr/bin/editor".into(), icon("editor"));
        assert_eq!(
            index[&PathBuf::from("/usr/bin/editor")],
            Some(icon("editor"))
        );
    }

    #[test]
    fn environment_wrappers_identify_only_the_program_they_launch() {
        let args = ["env", "LANG=en_US", "--", "/opt/Editor App/editor", "%U"].map(String::from);
        assert_eq!(program(&args), Some("/opt/Editor App/editor"));
        let unsupported = ["env", "-S", "python app.py"].map(String::from);
        assert_eq!(program(&unsupported), None);
    }

    #[test]
    fn script_and_sandbox_launchers_do_not_claim_every_process_of_their_runtime() {
        for arguments in [
            vec!["python3", "/opt/app/main.py"],
            vec!["env", "MODE=1", "node", "/opt/app/main.js"],
            vec!["flatpak", "run", "org.example.App"],
            vec!["sh", "-c", "start-app"],
            vec!["electron38", "/opt/app/app.asar"],
            vec!["mono", "/opt/app/app.exe"],
            vec!["dotnet", "/opt/app/app.dll"],
        ] {
            assert_eq!(
                program(&arguments.into_iter().map(String::from).collect::<Vec<_>>()),
                None
            );
        }
        assert_eq!(
            program(&["/usr/bin/python3".into()]),
            Some("/usr/bin/python3")
        );
    }

    #[test]
    fn canonical_runtime_aliases_do_not_identify_unrelated_processes() {
        let arguments = ["env", "MODE=1", "/opt/application-launcher", "main.py"].map(String::from);
        let command = command_executable(&arguments).unwrap();
        assert!(!command.identifies(std::path::Path::new("/usr/bin/python3.11")));
        assert!(!command.identifies(std::path::Path::new("/usr/bin/bash")));
        assert!(command.identifies(std::path::Path::new("/opt/editor")));
    }
}
