//! Resolve quit targets from the full current process set, never a displayed ranking.
use super::{
    process_control::{request_application_quit, ApplicationQuitStatus},
    running_identity,
};
use crate::{CoreError, CoreResult};
use strawberrydisk_platform::system_resources::memory::{
    MemorySampler, MemorySource, ProcessMemory,
};

pub fn request(application_id: &str) -> CoreResult<ApplicationQuitStatus> {
    if application_id.len() != 64 || !application_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::invalid_input("invalid application identity"));
    }
    let started = std::time::Instant::now();
    let processes = MemorySampler::default()
        .sample(true)?
        .processes
        .unwrap_or_default();
    let status = resolve_and_request(
        application_id,
        &processes,
        std::process::id(),
        request_application_quit,
    )?;
    log::info!(
        "application_quit_requested status={status:?} inspected={} elapsed_ms={}",
        processes.len(),
        started.elapsed().as_millis()
    );
    Ok(status)
}

fn resolve_and_request(
    id: &str,
    processes: &[ProcessMemory],
    current_pid: u32,
    request: impl FnOnce(&std::path::Path) -> CoreResult<ApplicationQuitStatus>,
) -> CoreResult<ApplicationQuitStatus> {
    let own_path = processes
        .iter()
        .find(|process| process.pid == current_pid)
        .and_then(|process| process.executable.as_deref())
        .map(running_identity::application_path);
    for process in processes {
        let path = process
            .executable
            .as_deref()
            .map(running_identity::application_path);
        if running_identity::id(path.as_deref(), process.pid) != id {
            continue;
        }
        let Some(path) = path.filter(|path| running_identity::can_quit(path, own_path.as_deref()))
        else {
            return Ok(ApplicationQuitStatus::Unsupported);
        };
        // Native adapters recheck the live identity before sending a normal quit request.
        return request(&path);
    }
    Ok(ApplicationQuitStatus::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn processes() -> Vec<ProcessMemory> {
        (1..=40)
            .map(|pid| ProcessMemory {
                pid,
                name: format!("App{pid}"),
                executable: Some(format!("/App{pid}.app/Contents/MacOS/App").into()),
                used_bytes: Some(if pid == 40 { 0 } else { 1000 - u64::from(pid) }),
                is_application: false,
            })
            .collect()
    }
    #[test]
    fn quit_ignores_rank_and_zero_memory_but_rejects_stale_and_own_identities() {
        let processes = processes();
        let id = running_identity::id(Some(std::path::Path::new("/App40.app")), 40);
        assert_eq!(
            resolve_and_request(&id, &processes, 1, |path| {
                assert_eq!(path, std::path::Path::new("/App40.app"));
                Ok(ApplicationQuitStatus::Requested)
            })
            .unwrap(),
            ApplicationQuitStatus::Requested
        );
        assert_eq!(
            resolve_and_request(&id, &processes[..39], 1, |_| panic!("stale target")).unwrap(),
            ApplicationQuitStatus::Unavailable
        );
        assert_eq!(
            resolve_and_request(&id, &processes, 40, |_| panic!("own target")).unwrap(),
            ApplicationQuitStatus::Unsupported
        );
        assert!(request("/arbitrary/path").is_err());
    }
}
