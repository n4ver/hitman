use hitman_lib::*;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_resolve_tf2_and_autoexec_paths() {
    let mut root = PathBuf::from(std::env::temp_dir());
    root.push("hitman_integration_test_root");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    // create custom folder inside root
    let custom = root.join("custom");
    fs::create_dir_all(&custom).unwrap();

    // resolve_tf2_root should return the parent (root)
    let resolved = resolve_tf2_root(&custom).expect("should resolve tf2 root");
    assert_eq!(resolved, root);

    // cfg dir should be root/cfg
    let cfg_dir = resolve_cfg_dir(&custom).expect("should resolve cfg dir");
    assert_eq!(cfg_dir, root.join("cfg"));

    // Create overrides autoexec and test resolution
    let overrides_autoexec = cfg_dir.join("overrides").join("autoexec.cfg");
    fs::create_dir_all(overrides_autoexec.parent().unwrap()).unwrap();
    fs::write(&overrides_autoexec, "// override").unwrap();

    let resolved_autoexec_default = resolve_autoexec_path_with_mode(&custom, None).unwrap();
    assert_eq!(resolved_autoexec_default, overrides_autoexec);

    // vanilla mode should point to cfg/autoexec.cfg
    let vanilla_autoexec = cfg_dir.join("autoexec.cfg");
    let resolved_vanilla = resolve_autoexec_path_with_mode(&custom, Some("vanilla")).unwrap();
    assert_eq!(resolved_vanilla, vanilla_autoexec);

    // mastercomfig mode should point to overrides/autoexec.cfg
    let resolved_master = resolve_autoexec_path_with_mode(&custom, Some("mastercomfig")).unwrap();
    assert_eq!(resolved_master, overrides_autoexec);

    // ensure_exec_line works on the vanilla path too
    let _ = fs::remove_file(&vanilla_autoexec);
    let added = ensure_exec_line(&vanilla_autoexec, "exec hitman.cfg").unwrap();
    assert!(added);
    let added_again = ensure_exec_line(&vanilla_autoexec, "exec hitman.cfg").unwrap();
    assert!(!added_again);

    // cleanup
    let _ = fs::remove_dir_all(&root);
}
