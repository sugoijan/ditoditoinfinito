#![cfg(all(feature = "native", target_os = "linux", target_arch = "x86_64"))]
//! Optional original-fixture gate. Uses the public installed-bundle/supervisor
//! APIs; no browser, Python, Node, external movie launcher or live acquisition.
use ddi_danoni_export::{assets::*, native::DirectorySource, supervisor::*, worker_build};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[test]
fn original_cpu_vulkan_pixels_and_actionscript_bitmap_reads() {
    let (Some(bundle), Some(fixtures)) = (
        std::env::var_os("DDI_DANONI_WORKER_BUNDLE"),
        std::env::var_os("DDI_DANONI_SYNTHETIC_FIXTURES"),
    ) else {
        eprintln!(
            "SKIP: set DDI_DANONI_WORKER_BUNDLE and DDI_DANONI_SYNTHETIC_FIXTURES for original CPU-renderer fixtures"
        );
        return;
    };
    let worker = worker_build::load_bundle(Path::new(&bundle), &mut || Ok(())).unwrap();
    let fixtures = PathBuf::from(fixtures);
    let root = std::env::temp_dir().join(format!("ddi-linux-rendering-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let mut originals = BTreeMap::new();
    for (name, hash) in [
        (
            "fixture.swf",
            "fdd04d732998a0a8dfc3eeec2e30e2f97d9806b3175d56b597ed89ee8d63b1e2",
        ),
        (
            "fixture-data.txt",
            "64a663cd5244acf3ec2dc22e90b040e87ba6d6fb6d1fb72ce392d764a1a67bda",
        ),
        (
            "bitmap.swf",
            "5ac2a0b1c0b5cd9a460c88dd5376ad4c7a3aa055ff8ecc1071f2bd10c9f0cabd",
        ),
    ] {
        let bytes = fs::read(fixtures.join(name)).unwrap();
        assert_eq!(sha256(&bytes), hash, "Unexpected original fixture {name}");
        originals.insert(name, bytes);
    }
    for (name, scale, bitmap) in [
        ("shapes-1", 1, false),
        ("shapes-4", 4, false),
        ("bitmap", 1, true),
    ] {
        let input = root.join(format!("{name}-input"));
        fs::create_dir(&input).unwrap();
        let movie = if bitmap { "bitmap.swf" } else { "fixture.swf" };
        fs::write(input.join(movie), &originals[movie]).unwrap();
        let mut config = json!({"movie":movie,"sourceUrl":"https://example.invalid/fixture.swf?query=retained","pageUrl":"https://example.invalid/work.html","flashVars":{"difficulty":"Hard"},"captureFrames":[1,2],"render":true,"renderScale":scale,"avmExecutionMs":10000,"renderer":{"backend":"vulkan","adapter":"software"},"resources":[]});
        if !bitmap {
            fs::write(
                input.join("fixture-data.txt"),
                &originals["fixture-data.txt"],
            )
            .unwrap();
            config["resources"] = json!([{"url":"https://example.invalid/fixture-data.txt","path":"fixture-data.txt","sha256":sha256(&originals["fixture-data.txt"])}]);
        }
        let config = serde_json::to_vec(&config).unwrap();
        fs::write(input.join("config.json"), &config).unwrap();
        let source = DirectorySource::new(BTreeMap::from([("original".into(), input)])).unwrap();
        let request = CaptureRequest {
            config: Binding {
                namespace: "original".into(),
                path: "config.json".into(),
                sha256: sha256(&config),
            },
            movie: Binding {
                namespace: "original".into(),
                path: movie.into(),
                sha256: sha256(&originals[movie]),
            },
            provenance: None,
            limits: Limits::default(),
        };
        let output = root.join(name);
        let result = capture(&request, &worker, &source, &output, &mut || Ok(()));
        if scale > 1 {
            assert_eq!(result.unwrap_err().code, "unsupported-native-render-scale");
            assert!(!output.exists());
            eprintln!(
                "{name}: enlarged Linux rendering rejects before staging; native scale only is admitted"
            );
            continue;
        }
        let result = result.unwrap();
        assert!(
            result.ready,
            "{name}: {:?}; {}",
            result.failure,
            fs::read_to_string(output.join("capture/stderr.txt")).unwrap_or_default()
        );
        let snapshot: Value =
            serde_json::from_slice(&fs::read(output.join("capture/snapshot.json")).unwrap())
                .unwrap();
        let renderer: Value =
            serde_json::from_slice(&fs::read(output.join("capture/renderer.json")).unwrap())
                .unwrap();
        assert_eq!(snapshot["renderer"], renderer);
        assert_eq!(renderer["selected"]["backend"], "vulkan");
        assert_eq!(renderer["selected"]["deviceType"], "Cpu");
        assert_eq!(renderer["selected"]["software"], true);
        for (frame, rgb) in [(1, [200u8, 80, 40]), (2, [40, 160, 80])] {
            let pixels = fs::read(output.join(format!("capture/frame-{frame}.ppm"))).unwrap();
            let header = format!("P6\n{} {}\n255\n", 320 * scale, 240 * scale);
            assert!(pixels.starts_with(header.as_bytes()));
            assert_eq!(pixels.len(), header.len() + 320 * 240 * scale * scale * 3);
            assert_eq!(&pixels[header.len()..header.len() + 3], &[17, 34, 51]);
            let at = header.len() + (50 * scale * 320 * scale + 50 * scale) * 3;
            assert_eq!(&pixels[at..at + 3], &rgb);
            if bitmap {
                let state = &snapshot["captures"][frame - 1]["state"];
                let roots = state["roots"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|r| r["level"] == 0)
                    .collect::<Vec<_>>();
                assert_eq!(roots.len(), 1);
                let object = &state["objects"][roots[0]["value"]["ref"].as_u64().unwrap() as usize];
                let values = object["properties"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|p| p["name"]["text"] == "bitmapColor")
                    .collect::<Vec<_>>();
                assert_eq!(values.len(), 1);
                assert_eq!(
                    values[0]["value"].as_f64(),
                    Some(u32::from_be_bytes([0, rgb[0], rgb[1], rgb[2]]) as f64)
                );
            }
        }
        if bitmap {
            assert_eq!(snapshot["audit"]["runtimeErrors"], 0);
        }
        eprintln!(
            "{name}: passed under Linux confinement; {}",
            renderer["selected"]["name"]
        );
    }
    fs::remove_dir_all(root).unwrap();
}
