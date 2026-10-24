//! Mock builder (backend-spec 5.3). Deterministic for a given seed: the seed is stored on the mock
//! so it can be rebuilt (review M7).

use std::collections::{BTreeMap, HashMap};

use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

pub const DEFAULT_N: usize = 20;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub question_id: i64,
    /// `COALESCE(FamilyId, Id)`.
    pub family: i64,
    pub component_id: i64,
    pub primary_concept: Option<i64>,
    pub is_open: bool,
    /// From UserQuestionStat.
    pub attempts: i32,
    /// Rights not counting guesses.
    pub rights: i32,
    /// Latest answer was Wrong, Blank or a guess.
    pub last_was_miss: bool,
}

impl Candidate {
    pub fn missed_before(&self) -> bool {
        self.attempts > self.rights
    }
}

#[derive(Debug, Clone)]
pub struct LayoutRow {
    pub component_id: i64,
    pub question_count: usize,
    pub open_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    pub n: Option<usize>,
    /// Real exam layout (spec 5.3 step 3): the deadline's per-lecture counts.
    pub layout: Option<Vec<LayoutRow>>,
    pub go_easy: bool,
    pub misses_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolShort {
    pub component_id: Option<i64>,
    pub wanted: usize,
    pub got: usize,
}

#[derive(Debug, Clone)]
pub struct Built {
    pub question_ids: Vec<i64>,
    pub warnings: Vec<PoolShort>,
}

/// Spec 5.3 step 4: missed before ×2; go easy: right once ×0.3, twice or more ×0.1 (they multiply).
pub fn weight(c: &Candidate, go_easy: bool) -> f64 {
    let mut w = 1.0;
    if c.missed_before() {
        w *= 2.0;
    }
    if go_easy {
        match c.rights {
            1 => w *= 0.3,
            r if r >= 2 => w *= 0.1,
            _ => {}
        }
    }
    w
}

/// Weighted sampling without replacement (Efraimidis-Spirakis: key = u^(1/w), take the largest).
fn sample<'a>(rng: &mut ChaCha8Rng, items: &[&'a Candidate], k: usize, go_easy: bool) -> Vec<&'a Candidate> {
    let mut keyed: Vec<(f64, &Candidate)> = items
        .iter()
        .map(|c| {
            let u: f64 = rng.gen_range(f64::MIN_POSITIVE..1.0);
            (u.powf(1.0 / weight(c, go_easy)), *c)
        })
        .collect();
    keyed.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.question_id.cmp(&b.1.question_id)));
    keyed.into_iter().take(k).map(|(_, c)| c).collect()
}

/// Spreads `k` over a candidate set round-robin by primary concept (spec 5.3: "then over primary
/// concepts"), each concept's turn a weighted pick.
fn sample_spread<'a>(rng: &mut ChaCha8Rng, items: &[&'a Candidate], k: usize, go_easy: bool) -> Vec<&'a Candidate> {
    let mut buckets: BTreeMap<Option<i64>, Vec<&Candidate>> = BTreeMap::new();
    for c in items {
        buckets.entry(c.primary_concept).or_default().push(c);
    }
    let mut queues: Vec<Vec<&Candidate>> = buckets.into_values().map(|b| sample(rng, &b, b.len(), go_easy)).collect();
    queues.shuffle(rng);
    let mut out = Vec::with_capacity(k);
    while out.len() < k && queues.iter().any(|q| !q.is_empty()) {
        for q in queues.iter_mut() {
            if out.len() == k {
                break;
            }
            if !q.is_empty() {
                out.push(q.remove(0));
            }
        }
    }
    out
}

pub fn build(candidates: &[Candidate], opts: &BuildOptions, seed: u64) -> Built {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    // Step 5: misses only.
    let pool: Vec<&Candidate> = candidates.iter().filter(|c| !opts.misses_only || c.last_was_miss).collect();

    // Step 2: one per family, the representative drawn by weight.
    let mut families: BTreeMap<i64, Vec<&Candidate>> = BTreeMap::new();
    for c in pool {
        families.entry(c.family).or_default().push(c);
    }
    let reps: Vec<&Candidate> = families.into_values().map(|f| sample(&mut rng, &f, 1, opts.go_easy)[0]).collect();

    let mut by_component: BTreeMap<i64, Vec<&Candidate>> = BTreeMap::new();
    for c in &reps {
        by_component.entry(c.component_id).or_default().push(c);
    }

    let mut picked: Vec<i64> = Vec::new();
    let mut warnings = Vec::new();
    match &opts.layout {
        // Step 3a: real layout, per lecture QuestionCount of which OpenCount open; fall back to
        // other types when there are too few open ones.
        Some(rows) => {
            for row in rows {
                let comp = by_component.get(&row.component_id).cloned().unwrap_or_default();
                let open: Vec<&Candidate> = comp.iter().copied().filter(|c| c.is_open).collect();
                let open_take = row.open_count.min(row.question_count);
                let mut chosen = sample_spread(&mut rng, &open, open_take, opts.go_easy);
                let rest: Vec<&Candidate> = comp.iter().copied().filter(|c| !chosen.iter().any(|x| x.question_id == c.question_id)).collect();
                let others: Vec<&Candidate> = rest.iter().copied().filter(|c| !c.is_open).collect();
                let need = row.question_count - chosen.len();
                let mut fill = sample_spread(&mut rng, &others, need, opts.go_easy);
                if fill.len() < need {
                    let leftovers: Vec<&Candidate> = rest.iter().copied().filter(|c| c.is_open && !fill.iter().any(|x| x.question_id == c.question_id)).collect();
                    fill.extend(sample_spread(&mut rng, &leftovers, need - fill.len(), opts.go_easy));
                }
                chosen.extend(fill);
                if chosen.len() < row.question_count {
                    warnings.push(PoolShort { component_id: Some(row.component_id), wanted: row.question_count, got: chosen.len() });
                }
                picked.extend(chosen.iter().map(|c| c.question_id));
            }
        }
        // Step 3b: n spread over lectures, then over concepts.
        None => {
            let n = opts.n.unwrap_or(DEFAULT_N);
            let comps: Vec<(i64, Vec<&Candidate>)> = by_component.into_iter().collect();
            let mut quotas: HashMap<i64, usize> = HashMap::new();
            // Round-robin quotas, capped by what each lecture has; spill leftovers to others.
            let mut remaining = n;
            while remaining > 0 {
                let mut gave = false;
                for (id, items) in &comps {
                    if remaining == 0 {
                        break;
                    }
                    let q = quotas.entry(*id).or_insert(0);
                    if *q < items.len() {
                        *q += 1;
                        remaining -= 1;
                        gave = true;
                    }
                }
                if !gave {
                    break;
                }
            }
            for (id, items) in &comps {
                let q = quotas.get(id).copied().unwrap_or(0);
                picked.extend(sample_spread(&mut rng, items, q, opts.go_easy).iter().map(|c| c.question_id));
            }
            if picked.len() < n {
                warnings.push(PoolShort { component_id: None, wanted: n, got: picked.len() });
            }
        }
    }
    picked.shuffle(&mut rng);
    Built { question_ids: picked, warnings }
}

/// MC options are shown in a shuffled order, stored on the mock (spec 5.3 step 6).
pub fn shuffled_positions(count: usize, seed: u64, question_id: i64) -> Vec<usize> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ (question_id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let mut order: Vec<usize> = (0..count).collect();
    order.shuffle(&mut rng);
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: i64, comp: i64) -> Candidate {
        Candidate { question_id: id, family: id, component_id: comp, primary_concept: None, is_open: false, attempts: 0, rights: 0, last_was_miss: false }
    }

    #[test]
    fn weights() {
        assert_eq!(weight(&c(1, 1), false), 1.0);
        let missed = Candidate { attempts: 2, rights: 1, ..c(1, 1) };
        assert_eq!(weight(&missed, false), 2.0);
        assert!((weight(&missed, true) - 0.6).abs() < 1e-9, "missed ×2 and right once ×0.3 multiply");
        let solid = Candidate { attempts: 3, rights: 3, ..c(1, 1) };
        assert!((weight(&solid, true) - 0.1).abs() < 1e-9);
    }

    #[test]
    fn deterministic_for_a_seed_and_spread_over_lectures() {
        let pool: Vec<Candidate> = (1..=30).map(|i| c(i, 1 + i % 3)).collect();
        let opts = BuildOptions { n: Some(9), ..Default::default() };
        let a = build(&pool, &opts, 42);
        assert_eq!(a.question_ids, build(&pool, &opts, 42).question_ids);
        assert_ne!(a.question_ids, build(&pool, &opts, 43).question_ids);
        assert_eq!(a.question_ids.len(), 9);
        for comp in 1..=3 {
            assert_eq!(a.question_ids.iter().filter(|id| 1 + *id % 3 == comp).count(), 3, "3 per lecture");
        }
        assert!(a.warnings.is_empty());
    }

    #[test]
    fn one_per_family() {
        let mut pool: Vec<Candidate> = (1..=5).map(|i| Candidate { family: 100, ..c(i, 1) }).collect();
        pool.push(c(6, 1));
        let b = build(&pool, &BuildOptions { n: Some(5), ..Default::default() }, 1);
        assert_eq!(b.question_ids.len(), 2, "five copies of one past-paper question count once");
        assert_eq!(b.warnings, vec![PoolShort { component_id: None, wanted: 5, got: 2 }]);
    }

    #[test]
    fn misses_only() {
        let mut pool: Vec<Candidate> = (1..=6).map(|i| c(i, 1)).collect();
        pool[0].last_was_miss = true;
        pool[3].last_was_miss = true;
        let b = build(&pool, &BuildOptions { n: Some(10), misses_only: true, ..Default::default() }, 7);
        let mut ids = b.question_ids.clone();
        ids.sort();
        assert_eq!(ids, vec![1, 4]);
    }

    #[test]
    fn real_layout_with_open_fallback() {
        let mut pool: Vec<Candidate> = (1..=6).map(|i| c(i, 1)).collect();
        pool[0].is_open = true;
        pool.extend((7..=8).map(|i| c(i, 2)));
        let layout = vec![LayoutRow { component_id: 1, question_count: 3, open_count: 2 }, LayoutRow { component_id: 2, question_count: 4, open_count: 0 }];
        let b = build(&pool, &BuildOptions { layout: Some(layout), ..Default::default() }, 3);
        assert!(b.question_ids.contains(&1), "the one open question is used");
        assert_eq!(b.question_ids.iter().filter(|id| **id <= 6).count(), 3, "short on open: filled with MC");
        assert_eq!(b.warnings, vec![PoolShort { component_id: Some(2), wanted: 4, got: 2 }]);
    }

    #[test]
    fn weighting_prefers_misses() {
        // 1 missed question among 50; over many seeds it's picked far more often than chance.
        let mut pool: Vec<Candidate> = (1..=50).map(|i| c(i, 1)).collect();
        pool[0].attempts = 1;
        let hits = (0..400).filter(|s| build(&pool, &BuildOptions { n: Some(5), ..Default::default() }, *s).question_ids.contains(&1)).count();
        // Unweighted: 5/50 × 400 = 40. Weight ×2 roughly doubles it.
        assert!((60..=110).contains(&hits), "picked {hits}/400; expected about 80");
    }

    #[test]
    fn option_shuffle_is_a_stable_permutation() {
        let a = shuffled_positions(4, 9, 77);
        assert_eq!(a, shuffled_positions(4, 9, 77));
        let mut s = a.clone();
        s.sort();
        assert_eq!(s, vec![0, 1, 2, 3]);
    }
}
