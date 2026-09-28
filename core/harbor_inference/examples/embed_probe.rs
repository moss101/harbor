//! Embedding sanity probe: cosine between clearly-related and
//! clearly-unrelated sentence pairs on a GGUF embedding model.
//! usage: cargo run -p harbor_inference --example embed_probe -- <model.gguf> [e5]
#[cfg(feature = "gguf-backend")]
mod probe {
    use harbor_inference::provider::{ModelProvider, ModelRef};
    use harbor_inference::GgufLlamaCppProvider;
    use harbor_modelhub::install::{
        PackageFile, PackageInstaller, PackageManifest, RuntimeBinding,
    };

    pub fn main() {
        let args: Vec<String> = std::env::args().collect();
        let path = args[1].clone();
        let e5 = args.get(2).map(|s| s == "e5").unwrap_or(false);
        let dir = tempfile::tempdir().unwrap();
        let models = dir.path().join("m");
        std::fs::create_dir_all(&models).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let sha = harbor_canonical::sha256_hex(&bytes);
        let id = format!("probe-{}", &sha[..8]);
        let file_name = std::path::Path::new(&path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let installer = PackageInstaller::new(&models);
        let m = PackageManifest {
            schema: "harbor.model/v3".into(),
            id: id.clone(),
            reference_type: "installed_package".into(),
            files: vec![PackageFile {
                role: "weights".into(),
                path: file_name,
                sha256: sha,
                size_bytes: bytes.len() as u64,
            }],
            runtime: RuntimeBinding {
                kind: "gguf/llama.cpp".into(),
                min_revision: "0.1.156".into(),
                targets: vec![std::env::consts::ARCH.into()],
            },
        };
        let mut staged = installer.begin(&id).unwrap();
        installer
            .ingest_file(&mut staged, &m.files[0], &bytes)
            .unwrap();
        installer
            .commit(&mut staged, &m, chrono::Utc::now())
            .unwrap();

        let provider = GgufLlamaCppProvider::new(&models).unwrap();
        let model = ModelRef::InstalledPackage { package_id: id };
        provider.load(&model).unwrap();
        let wrap = |t: &str, q: bool| -> String {
            if e5 {
                if q {
                    format!("query: {t}")
                } else {
                    format!("passage: {t}")
                }
            } else {
                t.to_string()
            }
        };
        let pairs = [
            (
                "The contract value is 5000 USD and ends in December.",
                "The contract is worth 5000 USD, ending December.",
            ),
            (
                "The contract value is 5000 USD and ends in December.",
                "Cats sleep most of the day and hunt at night.",
            ),
            (
                "قيمة العقد خمسة آلاف دولار.",
                "العقد بقيمة خمسة آلاف دولار أمريكي.",
            ),
            ("قيمة العقد خمسة آلاف دولار.", "القطط تنام معظم النهار."),
            (
                "Annual leave entitlement is 24 days.",
                "What is the CEO's favorite color?",
            ),
        ];
        for (a, b) in pairs {
            let va = provider.embed(&model, &[wrap(a, false)]).unwrap()[0].clone();
            let vb = provider.embed(&model, &[wrap(b, true)]).unwrap()[0].clone();
            let dot: f32 = va.iter().zip(&vb).map(|(x, y)| x * y).sum();
            let na = va.iter().map(|x| x * x).sum::<f32>().sqrt();
            let nb = vb.iter().map(|y| y * y).sum::<f32>().sqrt();
            println!("cos = {:.4}   {} | {}", dot / (na * nb), a, b);
        }
    }
}

#[cfg(not(feature = "gguf-backend"))]
fn main() {
    eprintln!("embed_probe needs --features gguf-backend");
}

#[cfg(feature = "gguf-backend")]
fn main() {
    probe::main();
}
