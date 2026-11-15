//! Embedder-free skill routing eval (opt-in): `skills.suggest` with only a
//! chat model installed, over 30 requests the router was tuned on and 20
//! it was not (PROBE_SET=held). Run:
//!   PROBE_MODEL=/path/model.gguf [PROBE_SET=held] cargo test --release \
//!     -p harbor_ffi --test route_eval -- --ignored --nocapture
#![cfg(feature = "gguf")]
use std::ffi::{CStr, CString};

use harbor_ffi::{
    harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free,
};

struct H(*mut harbor_ffi::WorkspaceHandle);
impl Drop for H {
    fn drop(&mut self) {
        unsafe { harbor_core_close(self.0) }
    }
}
impl H {
    fn raw(&self, method: &str, args: serde_json::Value) -> serde_json::Value {
        let req =
            CString::new(serde_json::json!({"method": method, "args": args}).to_string()).unwrap();
        let raw = unsafe { harbor_core_call(self.0, req.as_ptr()) };
        let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
        unsafe { harbor_core_string_free(raw) };
        serde_json::from_str(&text).unwrap()
    }
}

#[test]
#[ignore]
fn probe() {
    let model = std::env::var("PROBE_MODEL").expect("PROBE_MODEL");
    let dir = tempfile::tempdir().unwrap();
    let root = CString::new(dir.path().to_string_lossy().to_string()).unwrap();
    let ws = CString::new("ws-probe").unwrap();
    let hex =
        CString::new("3c1f6a0d8b7e4952a1d03f76c8e5b2194f60a7d3e81c5b92d4a6f0378e1c59ab").unwrap();
    let h = H(harbor_core_open_ex(
        root.as_ptr(),
        ws.as_ptr(),
        0,
        hex.as_ptr(),
    ));
    let v = h.raw(
        "models.install_from_path",
        serde_json::json!({"package_id": "chat", "path": model}),
    );
    assert_eq!(v["ok"], true, "{v}");
    // (request, has_file, expected skill id or "none")
    let all: &[(&str, bool, &str)] = &[
        ("create a cv for me in pdf, I am a systems analyst with 8 years at Aramco", false, "document-drafter"),
        ("make a presentation about our Q3 sales results", false, "presentation-builder"),
        ("I need a ppt for tomorrow's board meeting on hiring", false, "presentation-builder"),
        ("build a monthly budget spreadsheet with rent, food and transport", false, "sheet-builder"),
        ("write an email to Sara saying the shipment is delayed by two days", false, "email-drafting"),
        ("draft a cover letter for a data analyst job", false, "document-drafter"),
        ("summarise this meeting transcript: Ali: we ship Friday. Mona: QA needs Monday.", false, "meeting-notes"),
        ("weekly team update for the platform team, we shipped login and are blocked on API keys", false, "team-update"),
        ("prepare a project proposal for a new mobile app for clinics", false, "document-drafter"),
        ("turn this into slides", true, "report-to-slides"),
        ("fix the broken formulas in this workbook", true, "formula-audit"),
        ("clean up the duplicate rows", true, "table-cleanup"),
        ("اكتب سيرة ذاتية لمهندس برمجيات", false, "document-drafter"),
        ("أنشئ عرضا تقديميا عن الطاقة الشمسية", false, "presentation-builder"),
        // No keyword: the chat model decides.
        ("put together a deck-free pitch, just a one page write-up for investors about our startup", false, "document-drafter"),
        ("help me plan my expenses for the trip in a table with totals", false, "sheet-builder"),
        ("let my manager know I'll be off sick tomorrow", false, "email-drafting"),
        ("recap what we agreed in today's standup call: Omar owns the release, Lina fixes the crash", false, "meeting-notes"),
        ("I have a talk at the conference, prepare something I can show on screen about edge AI", false, "presentation-builder"),
        ("write a resignation for my employer", false, "document-drafter"),
        ("list the unpaid bills from these: INV-1 due 3 Oct 500 SAR, INV-2 due 9 Nov 200 SAR", false, "supplier-invoice-review"),
        // Not a task: answer in chat.
        ("tell me a joke", false, "none"),
        ("translate good morning to French", false, "none"),
        ("hi there", false, "none"),
        ("give me three tips for better sleep", false, "none"),
        ("write a short poem about the sea", false, "none"),
        ("calculate 15% of 2400", false, "none"),
        ("thanks, that was helpful", false, "none"),
        ("suggest a name for my bakery", false, "none"),
        ("convert 5 miles to km", false, "none"),
    ];
    let held: &[(&str, bool, &str)] = &[
        (
            "put together a slideshow on our hiring plan",
            false,
            "presentation-builder",
        ),
        (
            "I want to quit my job, write it up formally for HR",
            false,
            "document-drafter",
        ),
        (
            "ask the supplier for a refund on order 4471",
            false,
            "email-drafting",
        ),
        (
            "track my gym sessions and weights per week",
            false,
            "sheet-builder",
        ),
        (
            "make me a resume, I'm a nurse with 5 years in ICU",
            false,
            "document-drafter",
        ),
        (
            "prepare 5 slides introducing our new product to customers",
            false,
            "presentation-builder",
        ),
        (
            "reply to the client saying we accept the new dates",
            false,
            "email-drafting",
        ),
        (
            "write the notes from this call: we agreed to delay launch, Sam owns testing",
            false,
            "meeting-notes",
        ),
        (
            "a status update for the design team this week: finished icons, next is onboarding",
            false,
            "team-update",
        ),
        (
            "send a thank-you note to my interviewer",
            false,
            "email-drafting",
        ),
        (
            "write a complaint about the noisy neighbours to the building manager",
            false,
            "document-drafter",
        ),
        ("what's the capital of Japan", false, "none"),
        ("good morning!", false, "none"),
        ("explain how vaccines work in simple terms", false, "none"),
        ("recommend a book about habits", false, "none"),
        ("what is 12 times 19", false, "none"),
        ("summarise the plot of Hamlet", false, "none"),
        (
            "rewrite this sentence to sound friendlier: send me the file now",
            false,
            "none",
        ),
        ("ok cool", false, "none"),
        ("which is healthier, rice or quinoa", false, "none"),
    ];
    let set = std::env::var("PROBE_SET").unwrap_or_default();
    let all: &[(&str, bool, &str)] = if set == "held" { held } else { all };
    let only = std::env::var("PROBE_ONLY").ok();
    let cases: Vec<_> = all
        .iter()
        .filter(|c| only.as_deref().is_none_or(|o| c.0.contains(o)))
        .collect();
    let (mut ok, mut total) = (0, 0);
    for (req, has_file, want) in cases {
        let t = std::time::Instant::now();
        let v = h.raw(
            "skills.suggest",
            serde_json::json!({"text": req, "chat_package": "chat", "has_file": has_file}),
        );
        let r = &v["result"];
        let got = if r["abstained"] == false {
            r["ranked"][0]["skill_id"]
                .as_str()
                .unwrap_or("?")
                .to_string()
        } else if r["none_fit"] == true {
            "none".to_string()
        } else {
            format!("undecided:{}", r["llm_choice"])
        };
        total += 1;
        let pass = got == *want;
        if pass {
            ok += 1;
        }
        println!(
            "{} [{:>5}ms {:>8}] want {:<24} got {:<24} | {}",
            if pass { "PASS" } else { "FAIL" },
            t.elapsed().as_millis(),
            r["router"].as_str().unwrap_or("-"),
            want,
            got,
            req
        );
        if v["ok"] != true {
            println!("   error: {v}");
        }
    }
    println!("SCORE {ok}/{total}");
}
