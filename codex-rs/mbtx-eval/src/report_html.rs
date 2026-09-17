//! Self-contained, lazy-decoded report. Untrusted text is never executable HTML.
use std::io::Write;
use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::{Compression, write::GzEncoder};
use serde_json::{Value, json};

pub(crate) fn render(model: &Value) -> Result<String> {
    let mut summary = model.clone();
    let mut chunks = String::new();
    for (index, attempt) in summary["attempts"].as_array_mut().context("attempts")?.iter_mut().enumerate() {
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        serde_json::to_writer(&mut gzip, attempt)?;
        gzip.flush()?;
        chunks.push_str(&format!("<script type=\"application/octet-stream\" id=\"attempt-{index}\">{}</script>\n",STANDARD.encode(gzip.finish()?)));
        // Keep only filterable fields and counts in the initial parse. Every
        // recorded step remains in the per-attempt chunk, without a row cap.
        attempt.as_object_mut().context("attempt object")?.remove("details");
        let accounting = &attempt["accounting"];
        attempt["accounting"] = json!({"metrics":accounting["metrics"],"coverage":accounting["coverage"],"complete":accounting["complete"]});
        attempt.as_object_mut().unwrap().remove("tool_outcomes");
        attempt.as_object_mut().unwrap().remove("exchanges");
        attempt["chunk"] = json!(index);
    }
    // JSON inside a script element must escape the HTML parser's end tag.
    let data = serde_json::to_string(&summary)?.replace('<',"\\u003c").replace('>',"\\u003e").replace('&',"\\u0026");
    Ok(format!("{}<style>{}</style></head><body>{}<script type=\"application/json\" id=\"report-data\">{data}</script>{chunks}<script>{}</script><script>{}</script></body></html>",
        include_str!("../web/head.html"),include_str!("../web/report.css"),include_str!("../web/body.html"),include_str!("../web/vendor/echarts-6.0.0.min.js"),include_str!("../web/report.js")))
}
