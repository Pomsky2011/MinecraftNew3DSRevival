//! `JavaRandom` against vectors from the JDK's `java.util.Random` (tools/vectors/JavaRandomVectors.java).
//! Each seed's calls are replayed in order on one generator, so a single wrong step shows up.

use n3dsr_core::random::JavaRandom;

const VECTORS: &str = include_str!("data/java_random.txt");

#[test]
fn matches_java_util_random() {
    let mut random: Option<(i64, JavaRandom)> = None;
    let mut checked = 0;
    for (number, line) in VECTORS
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.starts_with('#'))
    {
        let at = format!("java_random.txt:{}: {line}", number + 1);
        let [seed, method, arg, expected] = line.split(' ').collect::<Vec<_>>()[..] else {
            panic!("{at}: not `seed method arg result`");
        };
        let seed: i64 = seed.parse().unwrap();
        // A new seed starts a new generator.
        if random.as_ref().map(|(s, _)| *s) != Some(seed) {
            random = Some((seed, JavaRandom::new(seed)));
        }
        let r = &mut random.as_mut().unwrap().1;
        let actual = match method {
            "nextInt" => r.next_int().to_string(),
            "nextIntBounded" => r.next_int_bounded(arg.parse().unwrap()).to_string(),
            "nextLong" => r.next_long().to_string(),
            "nextBoolean" => r.next_boolean().to_string(),
            "nextFloat" => format!("{:x}", r.next_float().to_bits()),
            "nextDouble" => format!("{:x}", r.next_double().to_bits()),
            _ => panic!("{at}: unknown method"),
        };
        assert_eq!(actual, expected, "{at}");
        checked += 1;
    }
    assert!(checked > 0, "no vectors");
}

#[test]
fn set_seed_restarts_the_sequence() {
    let mut r = JavaRandom::new(42);
    let first: Vec<i32> = (0..5).map(|_| r.next_int()).collect();
    r.set_seed(42);
    let again: Vec<i32> = (0..5).map(|_| r.next_int()).collect();
    assert_eq!(first, again);
}

#[test]
#[should_panic(expected = "bound must be positive")]
fn non_positive_bound_panics() {
    JavaRandom::new(0).next_int_bounded(0);
}
