//! Fold measurements using the shared resolved application identity before ranking.
use super::models::ApplicationIdentity;
use std::collections::BTreeMap;

// Bound published rankings after aggregating every readable process.
pub(super) const RANKING_LIMIT: usize = 50;

pub(super) fn aggregate<T>(
    rows: impl IntoIterator<Item = (ApplicationIdentity, T)>,
    merge: impl Fn(&mut T, T),
) -> Vec<(ApplicationIdentity, T)> {
    let mut groups = BTreeMap::<String, (ApplicationIdentity, T)>::new();
    for (mut identity, value) in rows {
        match groups.entry(identity.id.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                identity.process_count = 1;
                entry.insert((identity, value));
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let (identity, total) = entry.get_mut();
                identity.process_count += 1;
                merge(total, value);
            }
        }
    }
    groups.into_values().collect()
}
