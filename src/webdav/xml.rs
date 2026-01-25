pub fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn build_sync_collection_body(
    sync_token: Option<&str>,
    limit: Option<u32>,
    include_data: bool,
    namespace: &str,
    data_element: &str,
) -> String {
    let mut body = String::from(&format!(
        r#"<D:sync-collection xmlns:D="DAV:" xmlns:C="{namespace}">"#
    ));
    if let Some(token) = sync_token {
        body.push_str("<D:sync-token>");
        body.push_str(&escape_xml(token));
        body.push_str("</D:sync-token>");
    } else {
        body.push_str("<D:sync-token/>");
    }
    body.push_str("<D:sync-level>1</D:sync-level>");
    body.push_str("<D:prop><D:getetag/>");
    if include_data {
        body.push_str("<C:");
        body.push_str(data_element);
        body.push_str("/>");
    }
    body.push_str("</D:prop>");
    if let Some(limit) = limit {
        body.push_str("<D:limit><D:nresults>");
        body.push_str(&limit.to_string());
        body.push_str("</D:nresults></D:limit>");
    }
    body.push_str("</D:sync-collection>");
    body
}
