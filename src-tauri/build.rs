fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "system_status",
            "retry_worker",
            "worker_diagnostics",
            "wardrobe_categories",
            "wardrobe_state",
            "wardrobe_setup",
            "wardrobe_unlock",
            "wardrobe_change_passphrase",
            "wardrobe_list",
            "wardrobe_detail",
            "wardrobe_save",
            "wardrobe_delete",
            "wardrobe_image",
            "wardrobe_backups",
            "wardrobe_create_backup",
            "wardrobe_restore",
            "wardrobe_cancel_backup",
        ]),
    ))
    .expect("build FitSense desktop resources and permissions");
}
