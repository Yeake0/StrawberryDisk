//! Dynamic LaunchServices presentation names stay fresh; process counters are sampled separately.
use std::{collections::HashMap, path::PathBuf};

pub(super) struct ApplicationMetadata {
    pub name: Option<String>,
    pub executable: Option<PathBuf>,
}

pub(super) fn read() -> HashMap<u32, ApplicationMetadata> {
    objc2::rc::autoreleasepool(|_| {
        let applications = objc2_app_kit::NSWorkspace::sharedWorkspace().runningApplications();
        (0..applications.count())
            .filter_map(|index| {
                let application = applications.objectAtIndex(index);
                let pid = application.processIdentifier();
                if pid <= 0 || application.isTerminated() {
                    return None;
                }
                Some((
                    pid as u32,
                    ApplicationMetadata {
                        name: application
                            .localizedName()
                            .map(|name| name.to_string())
                            .filter(|name| !name.trim().is_empty()),
                        executable: application
                            .executableURL()
                            .and_then(|url| url.path())
                            .map(|path| PathBuf::from(path.to_string())),
                    },
                ))
            })
            .collect()
    })
}
