//! Native file-manager regression tests.
//!
//! Reveal-file command construction stays in a small module because each target OS
//! uses a different file-manager invocation.

use crate::app::app_platform::file_reveal_command;

#[test]
fn reveal_file_command_uses_platform_file_manager() {
    #[cfg(target_os = "windows")]
    {
        let path = std::path::PathBuf::from(r"C:\Videos\clip one.mp4");
        let command = file_reveal_command(&path);
        assert_eq!(command.program, "explorer");
        assert_eq!(command.args, vec![format!("/select,{}", path.display())]);
    }

    #[cfg(target_os = "macos")]
    {
        let path = std::path::PathBuf::from("/Users/me/Videos/clip one.mp4");
        let command = file_reveal_command(&path);
        assert_eq!(command.program, "open");
        assert_eq!(
            command.args,
            vec!["-R".to_string(), path.to_string_lossy().to_string()]
        );
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let path = std::path::PathBuf::from("/tmp/videos/clip one.mp4");
        let command = file_reveal_command(&path);
        assert_eq!(command.program, "xdg-open");
        assert_eq!(command.args, vec!["/tmp/videos".to_string()]);
    }
}
