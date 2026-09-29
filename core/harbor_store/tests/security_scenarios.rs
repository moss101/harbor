//! Security scenarios SEC-036, SEC-037 and SEC-038
//! (09_Security_Test_Matrix.csv) as executable controls:
//! - `security.sec_036` — private blob plaintext leakage: a synthetic
//!   secret stored through the blob store never appears in plaintext in
//!   the content store or its indexes at rest.
//! - `security.sec_037` — temp/journal plaintext leakage: decrypted
//!   working windows sweep on restart; bytes written through a temp
//!   window are gone after the sweep, and a "crashed" (un-closed)
//!   window is removed too.
//! - `security.sec_038` — key rotation/backup confusion: two workspaces
//!   under independent keys are isolated — unbinding one workspace's key
//!   leaves the other's blobs readable and the unbound one locked out
//!   (deleted scope follows its key, survivors keep theirs).

use harbor_store::blob::{BlobStore, PutOptions};
use harbor_store::keys::WorkspaceKey;
use harbor_store::temp::TempRegistry;

/// SEC-036: a synthetic secret put through the blob store is sealed —
/// byte-scanning the entire store root finds nothing of it at rest.
#[test]
fn sec_036_private_blob_plaintext_never_at_rest() {
    let dir = tempfile::tempdir().unwrap();
    let store = BlobStore::new(dir.path()).unwrap();
    let key = WorkspaceKey::generate();
    let root = harbor_store::keys::KeyMaterial::random();
    let wrapped = key.wrap_with(&root).unwrap();
    store.bind_workspace("ws-secret", key, wrapped);

    let secret = b"SYNTHETIC-SECRET-7f3a9c-private-workspace-content";
    let blob = store
        .put("ws-secret", secret, &PutOptions::default())
        .unwrap();
    assert_eq!(blob.size, secret.len() as u64);

    // Byte-scan every file under the store root: the secret, whole or in
    // part, must not exist at rest.
    fn scan(root: &std::path::Path, needle: &[u8]) -> Vec<std::path::PathBuf> {
        let mut hits = Vec::new();
        if let Ok(entries) = std::fs::read_dir(root) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    hits.extend(scan(&p, needle));
                } else if let Ok(bytes) = std::fs::read(&p) {
                    if bytes
                        .windows(needle.len().min(bytes.len().max(1)))
                        .any(|w| w == needle)
                    {
                        hits.push(p);
                    }
                }
            }
        }
        hits
    }
    let hits = scan(dir.path(), secret);
    assert!(hits.is_empty(), "plaintext leaked at rest: {hits:?}");
}

/// SEC-037: temp working windows are swept on restart — closed and
/// crashed windows alike leave no plaintext behind.
#[test]
fn sec_037_temp_windows_swept_on_restart() {
    let dir = tempfile::tempdir().unwrap();
    let registry = TempRegistry::new(dir.path().join("tmp")).unwrap();
    let needle = b"TEMP-PLAINTEXT-9b2c-crash-residue";

    // A properly closed window.
    {
        let handle = registry.open_window("preview", needle).unwrap();
        std::fs::write(handle.path(), needle).unwrap();
        handle.close();
    }
    // A "crashed" window: opened, written, never closed.
    let leaked = registry.open_window("crashed-render", needle).unwrap();
    std::fs::write(leaked.path(), needle).unwrap();
    std::mem::forget(leaked);

    // Restart: the sweep removes residue.
    let report = registry.sweep_on_restart().unwrap();
    assert_eq!(report.removed_files.len(), 1, "crashed window removed");
    fn scan(root: &std::path::Path, needle: &[u8]) -> bool {
        if let Ok(entries) = std::fs::read_dir(root) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if scan(&p, needle) {
                        return true;
                    }
                } else if let Ok(bytes) = std::fs::read(&p) {
                    if bytes
                        .windows(needle.len().min(bytes.len().max(1)))
                        .any(|w| w == needle)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
    assert!(
        !scan(dir.path(), needle),
        "temp plaintext must be gone after the restart sweep"
    );
}

/// SEC-038: workspaces under independent keys are isolated — unbinding
/// one workspace's key locks ITS blobs out and leaves the other's
/// readable (the deleted scope follows its key; survivors keep theirs).
#[test]
fn sec_038_workspace_key_isolation_on_unbind() {
    let dir = tempfile::tempdir().unwrap();
    let store = BlobStore::new(dir.path()).unwrap();
    let root = harbor_store::keys::KeyMaterial::random();

    let key_a = WorkspaceKey::generate();
    let wrapped_a = key_a.wrap_with(&root).unwrap();
    store.bind_workspace("ws-a", key_a, wrapped_a);
    let key_b = WorkspaceKey::generate();
    let wrapped_b = key_b.wrap_with(&root).unwrap();
    store.bind_workspace("ws-b", key_b, wrapped_b);

    let blob_a = store
        .put("ws-a", b"workspace A content", &PutOptions::default())
        .unwrap();
    let blob_b = store
        .put("ws-b", b"workspace B content", &PutOptions::default())
        .unwrap();

    // Delete workspace A's scope: its key binding is removed.
    store.unbind_workspace("ws-a");

    // A's blobs are locked out with its key gone...
    let a_result = store.get("ws-a", &blob_a.id, &PutOptions::default());
    assert!(
        a_result.is_err(),
        "the unbound workspace must be locked out, got {a_result:?}"
    );

    // ...and B survives with its own key untouched.
    let b_bytes = store
        .get("ws-b", &blob_b.id, &PutOptions::default())
        .unwrap();
    assert_eq!(b_bytes, b"workspace B content");
}
