#[cfg(feature = "simulator")]
mod directory_tests {
    use std::io::Write;
    use switchy_fs::{simulator::*, sync::*};

    fn setup_test() {
        // Reset filesystem before each test
        reset_fs();
    }

    #[test]
    fn test_create_and_list_directories() {
        setup_test();

        // Create nested directory structure
        create_dir_all("/tmp/test/nested").unwrap();
        create_dir_all("/home/user").unwrap();

        // List root directory
        let root_entries = read_dir_sorted("/").unwrap();
        let dir_names: Vec<String> = root_entries
            .iter()
            .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        assert!(dir_names.contains(&"tmp".to_string()));
        assert!(dir_names.contains(&"home".to_string()));

        // List nested directory
        let tmp_entries = read_dir_sorted("/tmp").unwrap();
        let tmp_dirs: Vec<String> = tmp_entries
            .iter()
            .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        assert!(tmp_dirs.contains(&"test".to_string()));
    }

    #[test]
    fn test_file_requires_parent_directory() {
        setup_test();

        // Trying to create a file without parent directory should fail
        let result = OpenOptions::new()
            .create(true)
            .write(true)
            .open("/nonexistent/file.txt");

        assert!(result.is_err());
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().kind(), std::io::ErrorKind::NotFound);

        // Create parent directory first
        create_dir_all("/tmp").unwrap();

        // Now file creation should succeed
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open("/tmp/file.txt")
            .unwrap();

        file.write_all(b"test content").unwrap();
    }

    #[test]
    fn test_read_dir_with_mixed_content() {
        setup_test();

        // Setup directory with files and subdirectories
        create_dir_all("/test").unwrap();
        create_dir_all("/test/subdir").unwrap();

        // Create some files
        let mut file1 = OpenOptions::new()
            .create(true)
            .write(true)
            .open("/test/file1.txt")
            .unwrap();
        file1.write_all(b"content1").unwrap();

        let mut file2 = OpenOptions::new()
            .create(true)
            .write(true)
            .open("/test/file2.txt")
            .unwrap();
        file2.write_all(b"content2").unwrap();

        // List directory contents
        let entries = read_dir_sorted("/test").unwrap();

        // Should have 2 files and 1 directory
        let files: Vec<_> = entries
            .iter()
            .filter(|e| e.file_type().is_ok_and(|ft| ft.is_file()))
            .collect();
        let dirs: Vec<_> = entries
            .iter()
            .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
            .collect();

        assert_eq!(files.len(), 2);
        assert_eq!(dirs.len(), 1);

        // Check specific names
        let file_names: Vec<String> = files
            .iter()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        let dir_names: Vec<String> = dirs
            .iter()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        assert!(file_names.contains(&"file1.txt".to_string()));
        assert!(file_names.contains(&"file2.txt".to_string()));
        assert!(dir_names.contains(&"subdir".to_string()));
    }

    #[test]
    fn test_walk_directory_tree() {
        setup_test();

        // Create complex directory structure
        create_dir_all("/root/level1/level2").unwrap();
        create_dir_all("/root/other").unwrap();

        // Add files at different levels
        OpenOptions::new()
            .create(true)
            .write(true)
            .open("/root/root_file.txt")
            .unwrap();

        OpenOptions::new()
            .create(true)
            .write(true)
            .open("/root/level1/level1_file.txt")
            .unwrap();

        OpenOptions::new()
            .create(true)
            .write(true)
            .open("/root/level1/level2/level2_file.txt")
            .unwrap();

        // Walk the entire tree
        let all_entries = walk_dir_sorted("/root").unwrap();

        // Should include all files and directories
        let paths: Vec<String> = all_entries
            .iter()
            .map(|e| e.path().to_string_lossy().to_string())
            .collect();

        // Check that all expected paths are present
        assert!(paths.iter().any(|p| p.contains("root_file.txt")));
        assert!(paths.iter().any(|p| p.contains("level1_file.txt")));
        assert!(paths.iter().any(|p| p.contains("level2_file.txt")));
        assert!(paths.iter().any(|p| p.ends_with("level1")));
        assert!(paths.iter().any(|p| p.ends_with("level2")));
        assert!(paths.iter().any(|p| p.ends_with("other")));
    }

    #[test]
    fn test_remove_directory_tree() {
        setup_test();

        // Create directory with content
        create_dir_all("/to_delete/sub1/sub2").unwrap();
        create_dir_all("/to_delete/sub3").unwrap();

        OpenOptions::new()
            .create(true)
            .write(true)
            .open("/to_delete/file.txt")
            .unwrap();

        OpenOptions::new()
            .create(true)
            .write(true)
            .open("/to_delete/sub1/file.txt")
            .unwrap();

        // Verify it exists
        assert!(read_dir_sorted("/to_delete").is_ok());

        // Remove the entire tree
        remove_dir_all("/to_delete").unwrap();

        // Should no longer exist
        assert!(read_dir_sorted("/to_delete").is_err());

        // Files should also be gone
        assert!(read_to_string("/to_delete/file.txt").is_err());
        assert!(read_to_string("/to_delete/sub1/file.txt").is_err());
    }

    #[test]
    fn test_init_filesystem_helpers() {
        setup_test();

        // Test minimal filesystem setup
        init_minimal_fs().unwrap();

        // Should have basic directories
        assert!(read_dir_sorted("/").is_ok());
        assert!(read_dir_sorted("/tmp").is_ok());
        assert!(read_dir_sorted("/home").is_ok());

        // Test user home setup
        init_user_home("testuser").unwrap();

        // Should have user directories
        assert!(read_dir_sorted("/home/testuser").is_ok());
        assert!(read_dir_sorted("/home/testuser/.config").is_ok());
        assert!(read_dir_sorted("/home/testuser/.local").is_ok());
        assert!(read_dir_sorted("/home/testuser/Documents").is_ok());

        // Reset and test standard filesystem
        reset_fs();
        init_standard_fs().unwrap();

        // Should have all FHS directories
        assert!(read_dir_sorted("/usr").is_ok());
        assert!(read_dir_sorted("/usr/bin").is_ok());
        assert!(read_dir_sorted("/var").is_ok());
        assert!(read_dir_sorted("/var/log").is_ok());
    }

    #[test]
    fn test_composable_filesystem_setup() {
        setup_test();

        // Build filesystem piece by piece
        init_minimal_fs().unwrap();
        init_user_home("alice").unwrap();
        init_user_home("bob").unwrap();

        create_dir_all("/opt/myapp").unwrap();

        // Verify all pieces exist
        assert!(read_dir_sorted("/").is_ok());
        assert!(read_dir_sorted("/home/alice").is_ok());
        assert!(read_dir_sorted("/home/bob").is_ok());
        assert!(read_dir_sorted("/opt/myapp").is_ok());

        // Can create files in user directories
        OpenOptions::new()
            .create(true)
            .write(true)
            .open("/home/alice/test.txt")
            .unwrap();

        // And in custom app directory
        OpenOptions::new()
            .create(true)
            .write(true)
            .open("/opt/myapp/config.json")
            .unwrap();
    }
}
