//! One computation at a time per key, and a result that is fresh enough is shared instead of computed again.
//! The window asks for the same statistics from several places every few seconds; when an answer takes longer
//! than the interval, asks must not pile up and compete for the same database.
use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

type Slot<V> = Arc<Mutex<Option<(Instant, V)>>>;
/// More distinct keys than this clear the finished entries; keys are a profile and a period chosen by a person.
const MAX_KEYS: usize = 64;

pub struct Coalesce<K, V> {
    slots: Mutex<HashMap<K, Slot<V>>>,
}
impl<K, V> Default for Coalesce<K, V> {
    fn default() -> Self {
        Self { slots: Mutex::new(HashMap::new()) }
    }
}
impl<K: Hash + Eq, V: Clone> Coalesce<K, V> {
    /// Returns the result computed at most `max_age` ago for `key`, or computes it. Callers that arrive while
    /// a computation for the same key runs wait for it and take its result.
    pub fn get(&self, key: K, max_age: Duration, compute: impl FnOnce() -> V) -> V {
        let slot = {
            let mut slots = self.slots.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if slots.len() >= MAX_KEYS && !slots.contains_key(&key) {
                // Only slots nobody holds go: a computation that is running keeps its slot, so the asks that
                // follow still share it.
                slots.retain(|_, slot| Arc::strong_count(slot) > 1);
            }
            slots.entry(key).or_default().clone()
        };
        let mut result = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((at, value)) = result.as_ref()
            && at.elapsed() <= max_age
        {
            return value.clone();
        }
        let value = compute();
        *result = Some((Instant::now(), value.clone()));
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn callers_that_arrive_while_a_computation_runs_share_its_result() {
        let table = Arc::new(Coalesce::<&'static str, usize>::default());
        let runs = Arc::new(AtomicUsize::new(0));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let (table, runs) = (table.clone(), runs.clone());
                std::thread::spawn(move || {
                    table.get("profile", Duration::from_secs(5), || {
                        std::thread::sleep(Duration::from_millis(150));
                        runs.fetch_add(1, Ordering::SeqCst) + 1
                    })
                })
            })
            .collect();
        let answers: Vec<usize> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(runs.load(Ordering::SeqCst), 1, "eight asks, one computation");
        assert!(answers.iter().all(|answer| *answer == 1));
    }

    #[test]
    fn an_old_result_is_computed_again_and_other_keys_do_not_share() {
        let table = Coalesce::<&'static str, usize>::default();
        let runs = AtomicUsize::new(0);
        let compute = || runs.fetch_add(1, Ordering::SeqCst) + 1;
        assert_eq!(table.get("a", Duration::from_millis(20), compute), 1);
        assert_eq!(table.get("a", Duration::from_millis(20), compute), 1, "still fresh");
        assert_eq!(table.get("b", Duration::from_millis(20), compute), 2, "another key");
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(table.get("a", Duration::from_millis(20), compute), 3, "too old");
    }

    #[test]
    fn a_computation_in_flight_keeps_its_slot_when_the_table_is_full() {
        let table = Arc::new(Coalesce::<usize, usize>::default());
        let runs = Arc::new(AtomicUsize::new(0));
        let slow = {
            let (table, runs) = (table.clone(), runs.clone());
            std::thread::spawn(move || {
                table.get(0, Duration::from_secs(5), || {
                    std::thread::sleep(Duration::from_millis(300));
                    runs.fetch_add(1, Ordering::SeqCst) + 1
                })
            })
        };
        std::thread::sleep(Duration::from_millis(50)); // key 0 is being computed
        for key in 1..=(MAX_KEYS * 2) {
            let _ = table.get(key, Duration::from_secs(5), || key);
        }
        let again = table.get(0, Duration::from_secs(5), || runs.fetch_add(1, Ordering::SeqCst) + 1);
        assert_eq!(slow.join().unwrap(), 1);
        assert_eq!(again, 1, "the second ask took the answer of the running computation");
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn a_failed_computation_is_shared_too_and_the_table_stays_bounded() {
        let table = Coalesce::<usize, Result<u8, &'static str>>::default();
        assert_eq!(table.get(0, Duration::from_secs(5), || Err("statistics_unavailable")), Err("statistics_unavailable"));
        assert_eq!(table.get(0, Duration::from_secs(5), || Ok(1)), Err("statistics_unavailable"));
        for key in 1..=200 {
            let _ = table.get(key, Duration::from_secs(5), || Ok(1));
        }
        assert!(table.slots.lock().unwrap().len() <= MAX_KEYS);
    }
}
