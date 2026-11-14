//! Chat harness eval (opt-in): answers to 36 tuned and 30 fresh
//! (PROBE_SET=held) questions with checkable answers, with the exact tools
//! on, or off with HARBOR_EVAL_RAW=1. Run:
//!   PROBE_MODEL=/path/model.gguf [PROBE_SET=held] [HARBOR_EVAL_RAW=1] \
//!     cargo test --release -p harbor_ffi --test harness_eval -- --ignored --nocapture
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
    fn ask(&self, q: &str) -> String {
        let v = self.raw(
            "op.start_generate",
            serde_json::json!({"question": q, "chat_package": "chat", "general": true, "max_tokens": 384}),
        );
        let op = v["result"]["op_id"]
            .as_str()
            .unwrap_or_else(|| panic!("{v}"))
            .to_string();
        loop {
            let s = self.raw("op.status", serde_json::json!({"op_id": op}));
            let st = &s["result"];
            match st["state"].as_str() {
                Some("completed") | Some("succeeded") | Some("done") => {
                    return st["result"]["answer"].as_str().unwrap_or("").to_string()
                }
                Some("failed") | Some("cancelled") => return format!("ERROR {st}"),
                _ => std::thread::sleep(std::time::Duration::from_millis(50)),
            }
        }
    }
}

fn norm(s: &str) -> String {
    let mut t = s.to_lowercase();
    // 1,234.5 -> 1234.5
    let chars: Vec<char> = t.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if *c == ','
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())
        {
            continue;
        }
        out.push(*c);
    }
    t = out;
    t
}

/// Pass when any accepted form appears as a whole token (numbers are not
/// matched inside longer numbers).
fn passes(answer: &str, accept: &[String]) -> bool {
    let a = norm(answer);
    accept.iter().any(|want| {
        let w = norm(want);
        let mut from = 0;
        while let Some(pos) = a[from..].find(&w) {
            let start = from + pos;
            let end = start + w.len();
            let before = a[..start].chars().last();
            let after = a[end..].chars().next();
            let numeric = w
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '-');
            let edge = |c: Option<char>| match c {
                None => true,
                Some(c) => {
                    if numeric {
                        !(c.is_ascii_digit()
                            || c == '.'
                                && a[end..].chars().nth(1).is_some_and(|d| d.is_ascii_digit()))
                    } else {
                        !c.is_alphanumeric()
                    }
                }
            };
            if edge(before) && edge(after) {
                return true;
            }
            from = end;
        }
        false
    })
}

#[test]
#[ignore]
fn probe() {
    let model = std::env::var("PROBE_MODEL").expect("PROBE_MODEL");
    let today = chrono::Local::now().date_naive();
    let days_to = |y: i32, m: u32, d: u32| {
        (chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap() - today)
            .num_days()
            .to_string()
    };
    use chrono::Datelike;
    let new_year = days_to(today.year() + 1, 1, 1);
    let plus100 = today + chrono::Duration::days(100);
    let plus100_text = vec![
        plus100.format("%B %-d, %Y").to_string(),
        plus100.format("%-d %B %Y").to_string(),
        plus100.format("%Y-%m-%d").to_string(),
        plus100.format("%B %-d").to_string(),
    ];
    let wd = chrono::NaiveDate::from_ymd_opt(2026, 12, 25)
        .unwrap()
        .format("%A")
        .to_string();
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let cases: Vec<(&str, &str, Vec<String>)> = vec![
        ("arith", "what is 3-7*5", s(&["-32"])),
        ("arith", "17*23", s(&["391"])),
        ("arith", "what is (144/12)+7^2", s(&["61"])),
        ("percent", "what is 15% of 2400", s(&["360"])),
        ("percent", "increase 80 by 25%", s(&["100"])),
        ("percent", "what percent is 45 of 180", s(&["25%", "25 %", "25 percent"])),
        ("percent", "a 250 SAR jacket is 30% off, what's the sale price", s(&["175"])),
        ("word", "I buy 3 shirts at 45 SAR each and get 10% off the total. How much do I pay?", s(&["121.5", "121.50"])),
        ("word", "A train travels 180 km in 2.5 hours. What is its average speed in km/h?", s(&["72"])),
        ("word", "Sara has 48 apples and gives a third of them to her brother, then eats 5. How many apples are left?", s(&["27"])),
        ("word", "If 4 workers build a wall in 6 days, how many days would 8 workers take?", s(&["3"])),
        ("word", "My salary is 12000 a month. I pay 3500 rent and spend 2000 on food. How much is left?", s(&["6500"])),
        ("word", "A recipe needs 250 g of flour for 4 people. How much flour is needed for 10 people?", s(&["625"])),
        ("word", "I drive 420 km and my car uses 7 litres per 100 km. How many litres do I need?", s(&["29.4"])),
        ("word", "A phone costs 3200 and I pay 400 per month. How many months until it is paid off?", s(&["8"])),
        ("units", "convert 5 miles to km", s(&["8.05", "8.04", "8.047"])),
        ("units", "how many pounds is 70 kg", s(&["154.3", "154.32", "154.4"])),
        ("units", "what is 100 F in celsius", s(&["37.8", "37.78", "37.7"])),
        ("units", "6 feet in meters", s(&["1.83", "1.829"])),
        ("units", "how many millilitres in 2.5 litres", s(&["2500"])),
        ("dates", "how many days until new year", vec![new_year.clone()]),
        ("dates", "what day of the week is 25 December 2026", vec![wd.clone()]),
        ("dates", "what date is 100 days from today", plus100_text.clone()),
        ("dates", "how many days between 1 March 2026 and 15 August 2026", s(&["167"])),
        ("dates", "what is today's date", vec![today.format("%B %-d").to_string(), today.format("%-d %B").to_string()]),
        ("text", "how many letters are in the word strawberry", s(&["10"])),
        ("text", "how many times does the letter r appear in strawberry", s(&["3"])),
        ("text", "how many words are in this sentence: the quick brown fox jumps over the lazy dog", s(&["9"])),
        ("text", "sort these numbers from smallest to largest: 42, 7, 19, 3, 88", s(&["3, 7, 19, 42, 88", "3,7,19,42,88"])),
        ("text", "spell the word harbor backwards", s(&["robrah"])),
        ("control", "what is the capital of France", s(&["paris"])),
        ("control", "who wrote Pride and Prejudice", s(&["austen"])),
        ("control", "what is the boiling point of water in celsius at sea level", s(&["100"])),
        ("control", "what is the chemical symbol for gold", s(&["au"])),
        ("control", "how many continents are there", s(&["7", "seven"])),
        ("control", "what is the largest planet in the solar system", s(&["jupiter"])),
    ];
    let held: Vec<(&str, &str, Vec<String>)> = vec![
        ("arith", "compute 1250 / 25 + 8", s(&["58"])),
        ("arith", "what's 19 squared minus 61", s(&["300"])),
        ("percent", "what's 7.5% of 640", s(&["48"])),
        ("percent", "decrease 1200 by 15%", s(&["1020"])),
        (
            "word",
            "Tickets cost 35 SAR for adults and 20 SAR for kids. How much for 2 adults and 3 kids?",
            s(&["130"]),
        ),
        (
            "word",
            "A 1.5 litre bottle fills glasses of 0.25 litres. How many glasses can I fill?",
            s(&["6"]),
        ),
        (
            "word",
            "I earn 85 per hour and work 38 hours a week. How much do I earn in a week?",
            s(&["3230"]),
        ),
        (
            "word",
            "A shop sells 12 eggs for 18 SAR. How much does one egg cost?",
            s(&["1.5", "1.50"]),
        ),
        (
            "word",
            "I read 24 pages a day. How many days will it take to finish a 360 page book?",
            s(&["15"]),
        ),
        (
            "word",
            "A laptop costs 4000 SAR plus 15% VAT. What is the total price?",
            s(&["4600"]),
        ),
        ("units", "convert 30 cm to inches", s(&["11.81", "11.8"])),
        (
            "units",
            "how many km is 26.2 miles",
            s(&["42.16", "42.2", "42.17"]),
        ),
        ("units", "convert 20 celsius to fahrenheit", s(&["68"])),
        (
            "units",
            "how many grams are in 3 pounds",
            s(&["1360.78", "1360.8", "1361"]),
        ),
        (
            "dates",
            "how many days till christmas",
            vec![days_to(today.year(), 12, 25)],
        ),
        ("dates", "what day was 1 January 2000", s(&["saturday"])),
        ("dates", "what is the date 3 weeks from now", {
            let d = today + chrono::Duration::days(21);
            vec![
                d.format("%B %-d").to_string(),
                d.format("%-d %B").to_string(),
                d.format("%Y-%m-%d").to_string(),
            ]
        }),
        (
            "dates",
            "how many days from 10 January 2026 to 20 February 2026",
            s(&["41"]),
        ),
        ("text", "how many letters in banana", s(&["6"])),
        ("text", "how many a's are in banana", s(&["3"])),
        ("text", "reverse the word ocean", s(&["naeco"])),
        (
            "text",
            "sort these numbers in descending order: 5, 12, 1, 9",
            s(&["12, 9, 5, 1", "12,9,5,1"]),
        ),
        (
            "text",
            "how many words are in the following text: I love on-device AI",
            s(&["4"]),
        ),
        (
            "control",
            "who painted the Mona Lisa",
            s(&["leonardo", "da vinci"]),
        ),
        ("control", "what is H2O commonly called", s(&["water"])),
        ("control", "how many days are in a week", s(&["7", "seven"])),
        (
            "control",
            "how many legs does a spider have",
            s(&["8", "eight"]),
        ),
        (
            "control",
            "what is the freezing point of water in fahrenheit",
            s(&["32"]),
        ),
        ("control", "which ocean is the largest", s(&["pacific"])),
        (
            "control",
            "what language is spoken in Brazil",
            s(&["portuguese"]),
        ),
    ];
    let cases = if std::env::var("PROBE_SET").as_deref() == Ok("held") {
        held
    } else {
        cases
    };
    let dir = tempfile::tempdir().unwrap();
    let root = CString::new(dir.path().to_string_lossy().to_string()).unwrap();
    let ws = CString::new("ws-hprobe").unwrap();
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
    let mut by_cat: std::collections::BTreeMap<&str, (u32, u32)> = Default::default();
    for (cat, q, accept) in &cases {
        let t = std::time::Instant::now();
        let a = h.ask(q);
        let ok = passes(&a, accept);
        let e = by_cat.entry(cat).or_default();
        e.1 += 1;
        if ok {
            e.0 += 1;
        }
        let shown: String = a.replace('\n', " ⏎ ").chars().take(150).collect();
        println!(
            "{} [{cat:<7} {:>5}ms] {q}\n      want {:?}\n      got  {shown}",
            if ok { "PASS" } else { "FAIL" },
            t.elapsed().as_millis(),
            accept
        );
    }
    let (mut ok, mut n) = (0, 0);
    for (cat, (o, t)) in &by_cat {
        println!("CAT {cat:<8} {o}/{t}");
        ok += o;
        n += t;
    }
    println!("SCORE {ok}/{n}");
}
