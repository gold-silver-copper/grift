#![allow(clippy::unwrap_used, clippy::expect_used)]

use grift::Grift;

#[test]
fn reclaimed_bytes_are_stale_value_to_the_host() {
    let mut g: Grift = Grift::new().unwrap();
    let original = g.bytes(&[0, 255, 195, 169]).unwrap();
    assert_eq!(
        g.as_bytes(&original).unwrap(),
        Some(&[0, 255, 195, 169][..])
    );

    // As in embedding.rs, enough allocation for several collections; no root keeps original.
    g.evaluate(
        &mut (),
        "(length (map (lambda! (n) (list n)) (range 0 20000)))",
    )
    .unwrap();
    let replacement = g.bytes(&[1, 2, 3, 4]).unwrap();
    assert_eq!(g.as_bytes(&replacement).unwrap(), Some(&[1, 2, 3, 4][..]));
    let error = g
        .as_bytes(&original)
        .expect_err("reclaimed bytes must not expose reused storage or return None");
    assert!(g.is_kind(&error, "stale-value").unwrap());
}

#[test]
fn another_interpreters_bytes_are_foreign_value_to_the_host() {
    // Matching allocation histories must not make another interpreter's reference local.
    let mut theirs: Grift = Grift::new().unwrap();
    let mut ours: Grift = Grift::new().unwrap();
    let foreign = theirs.bytes(&[0, 255]).unwrap();
    let local = ours.bytes(&[17, 34]).unwrap();
    assert_eq!(theirs.as_bytes(&foreign).unwrap(), Some(&[0, 255][..]));
    assert_eq!(ours.as_bytes(&local).unwrap(), Some(&[17, 34][..]));

    let error = ours
        .as_bytes(&foreign)
        .expect_err("another interpreter's bytes must not expose local storage or return None");
    assert!(ours.is_kind(&error, "foreign-value").unwrap());
}
