//! Self-contained, lazy-decoded report. Untrusted text is never executable HTML.
use anyhow::Context;
use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use flate2::Compression;
use flate2::GzBuilder;
use serde_json::Value;
use serde_json::json;
use std::io::Write;

pub(crate) fn render(model: &Value) -> Result<String> {
    let mut summary = serde_json::Map::new();
    for (key, value) in model.as_object().context("report object")? {
        if key != "attempts" {
            summary.insert(key.clone(), value.clone());
        }
    }
    let mut summaries = Vec::new();
    let mut chunks = String::new();
    for (index, attempt) in model["attempts"]
        .as_array()
        .context("attempts")?
        .iter()
        .enumerate()
    {
        let mut canonical = attempt.clone();
        canonical.sort_all_objects();
        let mut gzip = GzBuilder::new()
            .mtime(0)
            .write(Vec::new(), Compression::default());
        serde_json::to_writer(&mut gzip, &canonical)?;
        gzip.flush()?;
        chunks.push_str(&format!(
            "<script type=\"application/octet-stream\" id=\"attempt-{index}\">{}</script>\n",
            STANDARD.encode(gzip.finish()?)
        ));
        // Keep only filterable fields and counts in the initial parse. Every
        // recorded step remains in the per-attempt chunk, without a row cap.
        let mut brief = serde_json::Map::new();
        for (key, value) in attempt.as_object().context("attempt object")? {
            if !matches!(
                key.as_str(),
                "details" | "accounting" | "tool_outcomes" | "exchanges" | "interaction"
            ) {
                brief.insert(key.clone(), value.clone());
            }
        }
        let accounting = &attempt["accounting"];
        brief.insert("accounting".into(), json!({"metrics":accounting["metrics"],"observed":accounting["observed"],"coverage":accounting["coverage"],"complete":accounting["complete"]}));
        let mut outcomes = attempt["tool_outcomes"].clone();
        if let Some(outcomes) = outcomes.as_object_mut() {
            outcomes.remove("details");
        }
        brief.insert("tool_outcomes".into(), outcomes);
        let mut interaction = attempt["interaction"].clone();
        if let Some(interaction) = interaction.as_object_mut() {
            interaction.remove("decisions");
        }
        if !interaction.is_null() {
            brief.insert("interaction".into(), interaction);
        }
        brief.insert("chunk".into(), json!(index));
        summaries.push(Value::Object(brief));
    }
    summary.insert("attempts".into(), json!(summaries));
    let mut summary = Value::Object(summary);
    summary.sort_all_objects();
    // JSON inside a script element must escape the HTML parser's end tag.
    let data = serde_json::to_string(&summary)?
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    Ok(format!(
        "{}<style>{}</style></head><body>{}<script type=\"application/json\" id=\"report-data\">{data}</script>{chunks}<script>{}</script><script>{}</script></body></html>",
        include_str!("../web/head.html"),
        include_str!("../web/report.css"),
        include_str!("../web/body.html"),
        include_str!("../web/vendor/echarts-6.0.0.min.js"),
        include_str!("../web/report.js")
    ))
}
