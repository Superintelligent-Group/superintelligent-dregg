use criterion::{Criterion, black_box, criterion_group, criterion_main};
use pyana_bridge::{
    BridgePresentationBuilder, authorize_with_trace, macaroon_to_factset,
};
use pyana_bridge::present::{bytes_to_babybear, hash_index};
use pyana_circuit::BabyBear;
use pyana_circuit::merkle_air::MerkleAir;
use pyana_token::{Attenuation, AuthRequest, AuthToken, MacaroonToken};

// =============================================================================
// Helpers
// =============================================================================

fn test_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    key[0] = 0x42;
    key[1] = 0x13;
    key[31] = 0xFF;
    key
}

/// Compute the federation root that matches the synthetic Merkle path for the key.
fn compute_matching_federation_root(key: &[u8; 32]) -> (BabyBear, [u8; 32]) {
    let issuer_hash = bytes_to_babybear(key);
    let depth = 8;
    let mut current = issuer_hash;
    for i in 0..depth {
        let position = (i % 4) as u8;
        let siblings = [
            BabyBear::new(hash_index(i, 0, key)),
            BabyBear::new(hash_index(i, 1, key)),
            BabyBear::new(hash_index(i, 2, key)),
        ];
        current = MerkleAir::compute_parent(current, position, &siblings);
    }

    let mut fed_root_bytes = [0u8; 32];
    fed_root_bytes[..4].copy_from_slice(&current.0.to_le_bytes());
    (current, fed_root_bytes)
}

fn make_builder_and_request() -> (BridgePresentationBuilder, AuthRequest) {
    let key = test_key();
    let (fed_root_bb, fed_root_bytes) = compute_matching_federation_root(&key);

    let mut builder = BridgePresentationBuilder::new_with_root_bb(key, fed_root_bytes, fed_root_bb);
    let token = MacaroonToken::mint(key, b"kid-1", "pyana.dev");
    builder.set_root_token(token);

    let att = Attenuation {
        apps: vec![("my-app".into(), "rw".into())],
        ..Default::default()
    };
    builder.add_attenuation(&att);

    let request = AuthRequest {
        app_id: Some("my-app".into()),
        action: Some("r".into()),
        ..Default::default()
    };

    (builder, request)
}

// =============================================================================
// Benchmarks
// =============================================================================

fn bench_prove_mock(c: &mut Criterion) {
    c.bench_function("bridge_prove_mock", |b| {
        b.iter(|| {
            let (mut builder, request) = make_builder_and_request();
            black_box(builder.prove(&request).unwrap());
        });
    });
}

fn bench_prove_real(c: &mut Criterion) {
    c.bench_function("bridge_prove_real_stark", |b| {
        b.iter(|| {
            let (mut builder, request) = make_builder_and_request();
            black_box(builder.prove_real(&request).unwrap());
        });
    });
}

fn bench_prove_ivc(c: &mut Criterion) {
    c.bench_function("bridge_prove_ivc", |b| {
        b.iter(|| {
            let (mut builder, request) = make_builder_and_request();
            black_box(builder.prove_ivc(&request).unwrap());
        });
    });
}

fn bench_macaroon_to_factset_bench(c: &mut Criterion) {
    let key = test_key();
    let token = MacaroonToken::mint(key, b"kid-1", "pyana.dev");

    c.bench_function("bridge_macaroon_to_factset", |b| {
        b.iter(|| {
            black_box(macaroon_to_factset(&token));
        });
    });
}

fn bench_authorize_with_trace_bench(c: &mut Criterion) {
    let key = test_key();
    let (fed_root_bb, fed_root_bytes) = compute_matching_federation_root(&key);
    let mut builder = BridgePresentationBuilder::new_with_root_bb(key, fed_root_bytes, fed_root_bb);
    let token = MacaroonToken::mint(key, b"kid-1", "pyana.dev");
    builder.set_root_token(token);

    let att = Attenuation {
        apps: vec![("my-app".into(), "rw".into())],
        ..Default::default()
    };
    builder.add_attenuation(&att);

    let symbols = builder.symbols().clone();
    let final_state = builder.final_state().unwrap().clone();

    let request = AuthRequest {
        app_id: Some("my-app".into()),
        action: Some("r".into()),
        ..Default::default()
    };

    c.bench_function("bridge_authorize_with_trace", |b| {
        b.iter(|| {
            black_box(authorize_with_trace(&final_state, &request, &symbols).unwrap());
        });
    });
}

fn bench_end_to_end_cycle(c: &mut Criterion) {
    let key = test_key();

    c.bench_function("bridge_end_to_end_mint_attenuate_prove_verify", |b| {
        b.iter(|| {
            // Mint
            let token = MacaroonToken::mint(key, b"kid-1", "pyana.dev");

            // Attenuate
            let att = Attenuation {
                apps: vec![("my-app".into(), "rw".into())],
                ..Default::default()
            };
            let _attenuated = token.attenuate(&att).unwrap();

            // Build presentation
            let (fed_root_bb, fed_root_bytes) = compute_matching_federation_root(&key);
            let mut builder = BridgePresentationBuilder::new_with_root_bb(key, fed_root_bytes, fed_root_bb);
            builder.set_root_token(token);
            builder.add_attenuation(&att);

            let request = AuthRequest {
                app_id: Some("my-app".into()),
                action: Some("r".into()),
                ..Default::default()
            };
            let proof = builder.prove(&request).unwrap();

            // Verify
            black_box(proof.is_valid());
        });
    });
}

fn bench_verify_presentation(c: &mut Criterion) {
    let (mut builder, request) = make_builder_and_request();
    let proof = builder.prove(&request).unwrap();

    c.bench_function("bridge_verify_presentation", |b| {
        b.iter(|| {
            black_box(proof.is_valid());
        });
    });
}

criterion_group!(
    benches,
    bench_prove_mock,
    bench_prove_real,
    bench_prove_ivc,
    bench_macaroon_to_factset_bench,
    bench_authorize_with_trace_bench,
    bench_end_to_end_cycle,
    bench_verify_presentation,
);
criterion_main!(benches);
