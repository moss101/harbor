//! Scheduled goals through the FFI boundary (decision 0011): create with
//! validation, due/claim dedupes across restarts, state transitions,
//! durable trail, and the whole store sealed at rest (policy 13).

use std::ffi::{CStr, CString};

use harbor_ffi::{
    harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free,
};

struct Handle(*mut harbor_ffi::WorkspaceHandle);

impl Handle {
    fn open(root: &std::path::Path) -> Self {
        let root = CString::new(root.to_string_lossy().to_string()).unwrap();
        let ws = CString::new("ws-goals").unwrap();
        let root_hex =
            CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10")
                .unwrap();
        let h = harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, root_hex.as_ptr());
        assert!(!h.is_null(), "open failed");
        Handle(h)
    }

    fn raw(&self, method: &str, args: serde_json::Value) -> serde_json::Value {
        let req =
            CString::new(serde_json::json!({"method": method, "args": args}).to_string()).unwrap();
        let raw = unsafe { harbor_core_call(self.0, req.as_ptr()) };
        let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
        unsafe { harbor_core_string_free(raw) };
        serde_json::from_str(&text).expect("response parses")
    }

    fn ok(&self, method: &str, args: serde_json::Value) -> serde_json::Value {
        let v = self.raw(method, args);
        assert!(
            v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false),
            "{method}: {v}"
        );
        v["result"].clone()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { harbor_core_close(self.0) };
    }
}

fn every(minutes: u32) -> serde_json::Value {
    serde_json::json!({"kind": "every_minutes", "minutes": minutes})
}

#[test]
fn goals_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("data");
    std::fs::create_dir_all(&root).unwrap();
    let h = Handle::open(&root);

    // Create: validation errors are typed, not silent.
    let bad = h.raw(
        "goals.create",
        serde_json::json!({"title": "", "request": {"kind": "prompt", "text": "x"}, "schedule": every(10)}),
    );
    assert!(!bad.get("ok").and_then(|x| x.as_bool()).unwrap_or(false));

    let created = h.ok(
        "goals.create",
        serde_json::json!({
            "title": "Weekly contract digest",
            "request": {"kind": "skill", "skill_id": "thread-summary", "input": "summarize this week"},
            "schedule": every(10),
            "max_runs": 2,
        }),
    );
    let goal_id = created["id"].as_str().unwrap().to_string();
    assert!(goal_id.starts_with("goal-"));
    assert_eq!(created["state"], "active");

    // Due now: one entry with a deterministic slot.
    let due = h.ok("goals.due", serde_json::json!({}));
    let entries = due["due"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    let slot = entries[0]["slot"].as_str().unwrap().to_string();
    assert!(slot.starts_with("every-10-"));

    // Claim: reserves the slot and pins the run id the driver must
    // execute under. A second claim of the SAME slot is refused
    // (restart / duplicate driver) — at-most-once.
    let claim = h.ok(
        "goals.claim",
        serde_json::json!({"goal_id": goal_id, "slot": slot}),
    );
    let run_id = claim["run_id"].as_str().unwrap().to_string();
    assert!(run_id.starts_with("run-"));
    let dup = h.raw(
        "goals.claim",
        serde_json::json!({"goal_id": goal_id, "slot": slot}),
    );
    assert!(!dup.get("ok").and_then(|x| x.as_bool()).unwrap_or(false));
    // The durable run appears only when the driver executes the claim:
    // until then the receipt exists, the run does not.
    let pre = h.raw("run.state", serde_json::json!({"run_id": run_id}));
    assert!(!pre.get("ok").and_then(|x| x.as_bool()).unwrap_or(false));

    // Execute the claim for real: a model-free skill run UNDER the
    // claimed run id — the goal receipt and the durable run are then
    // the same identity (placeholder-fill needs no chat model).
    use base64::Engine as _;
    let tpl = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/office/letter_template.docx"),
    )
    .unwrap();
    let start = h.ok(
        "op.start_skill_run",
        serde_json::json!({
            "skill_id": "placeholder-fill",
            "run_id": run_id,
            "inputs": {"artifact_id": "tpl", "values": {"name": "Amina", "ref": "HB-42", "AMOUNT": "1,250.00", "sender": "Harbor Team"}},
            "artifacts": [{"id": "tpl", "name": "letter_template.docx", "data_b64": base64::engine::general_purpose::STANDARD.encode(&tpl)}]
        }),
    );
    let op_id = start["op_id"].as_str().unwrap().to_string();
    let mut report = serde_json::Value::Null;
    for _ in 0..600 {
        let st = h.ok("op.status", serde_json::json!({"op_id": op_id}));
        if st["state"] == "running" {
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }
        report = st["result"].clone();
        break;
    }
    assert_eq!(
        report["run_id"], run_id,
        "the execution used the claimed run id: {report}"
    );

    // Outcome attaches to the claim.
    h.ok(
        "goals.record_outcome",
        serde_json::json!({"goal_id": goal_id, "run_id": run_id, "outcome": "waiting_approval"}),
    );

    // max_runs=2: the goal is Done after the second slot's claim.
    let later_slot = format!("every-10-{}", 999_999);
    h.ok(
        "goals.claim",
        serde_json::json!({"goal_id": goal_id, "slot": later_slot}),
    );
    let goal = h.ok("goals.get", serde_json::json!({"id": goal_id}));
    assert_eq!(goal["goal"]["state"], "done");
    assert_eq!(goal["goal"]["run_count"], 2);
    assert_eq!(goal["goal"]["executions"].as_array().unwrap().len(), 2);

    // Terminal states hold; pause/resume/cancel path.
    let g2 = h.ok(
        "goals.create",
        serde_json::json!({
            "title": "Daily standup notes",
            "request": {"kind": "prompt", "text": "draft the standup update"},
            "schedule": every(1440),
        }),
    );
    let g2id = g2["id"].as_str().unwrap().to_string();
    let paused = h.ok("goals.pause", serde_json::json!({"goal_id": g2id}));
    assert_eq!(paused["state"], "paused");
    let resumed = h.ok("goals.resume", serde_json::json!({"goal_id": g2id}));
    assert_eq!(resumed["state"], "active");
    let cancelled = h.ok("goals.cancel", serde_json::json!({"goal_id": g2id}));
    assert_eq!(cancelled["state"], "cancelled");
    // Cancelled is terminal.
    let zombie = h.raw("goals.resume", serde_json::json!({"goal_id": g2id}));
    assert!(!zombie.get("ok").and_then(|x| x.as_bool()).unwrap_or(false));

    // A cancelled goal is never due.
    let due2 = h.ok("goals.due", serde_json::json!({}));
    assert!(due2["due"].as_array().unwrap().is_empty());

    // Policy 13: the goal store on disk is sealed — no prompt text.
    drop(h);
    let goals_file = std::fs::read_to_string(root.join("db").join("goals.json")).unwrap();
    assert!(goals_file.starts_with("enc.v1:"), "goal store not sealed");
    assert!(!goals_file.contains("standup"));

    // Reopen: goals survive with their receipts.
    let h2 = Handle::open(&root);
    let list = h2.ok("goals.list", serde_json::json!({}));
    let goals = list["goals"].as_array().unwrap();
    assert_eq!(goals.len(), 2);
    let survived = goals.iter().find(|g| g["id"] == goal_id).unwrap();
    assert_eq!(survived["state"], "done");
    assert_eq!(
        survived["executions"][0]["outcome"],
        serde_json::json!("waiting_approval")
    );
}
