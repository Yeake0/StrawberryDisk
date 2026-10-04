use mangodisk_core::DiskInfo;
use mangodisk_platform::{current_platform, Platform};

use super::error::{run_blocking, CommandResult};

#[tauri::command]
pub async fn get_system_disk(refresh: Option<bool>) -> CommandResult<DiskInfo> {
    run_blocking("get_system_disk", move || {
        if refresh.unwrap_or(false) {
            mangodisk_platform::system_resources::disk::invalidate_capacity_cache();
        }
        let disk = current_platform().system_volume().map(DiskInfo::from)?;
        log::info!(
            "system_disk_capacity_read refresh={} total_bytes={} available_bytes={} used_bytes={}",
            refresh.unwrap_or(false),
            disk.total_bytes,
            disk.available_bytes,
            disk.used_bytes
        );
        Ok::<_, mangodisk_platform::PlatformError>(disk)
    })
    .await
}

#[tauri::command]
pub async fn list_disks() -> CommandResult<Vec<DiskInfo>> {
    run_blocking("list_disks", || {
        current_platform()
            .volumes()
            .map(|volumes| volumes.into_iter().map(DiskInfo::from).collect())
    })
    .await
}
