fn article_html(body: &str, inline_urls: &[String]) -> String {
    let mut html = body
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                "<p><br/></p>".to_owned()
            } else {
                format!("<p>{}</p>", escape_html(line))
            }
        })
        .collect::<String>();
    for url in inline_urls {
        html.push_str(&format!(
            "<p><img src=\"{}\"/></p>",
            escape_html_attribute(url)
        ));
    }
    html
}

fn article_html_with_limit(body: &str, inline_urls: &[String]) -> Option<(String, bool)> {
    let source_chars = body.chars().count();
    let empty = article_html("", inline_urls);
    if !wechat_content_fits(&empty) {
        return None;
    }
    let full = article_html(body, inline_urls);
    if wechat_content_fits(&full) {
        return Some((full, false));
    }
    let mut lower = 0;
    let mut upper = source_chars;
    while lower < upper {
        let candidate_length = lower + (upper - lower).div_ceil(2);
        let candidate_body = truncate_characters(body, candidate_length);
        if wechat_content_fits(&article_html(&candidate_body, inline_urls)) {
            lower = candidate_length;
        } else {
            upper = candidate_length - 1;
        }
    }
    Some((
        article_html(&truncate_characters(body, lower), inline_urls),
        true,
    ))
}

fn wechat_content_fits(content: &str) -> bool {
    content.chars().count() <= WECHAT_MAX_ARTICLE_CHARS
        && content.len() <= WECHAT_MAX_ARTICLE_BYTES
}

fn truncate_characters(value: &str, maximum: usize) -> String {
    value.chars().take(maximum).collect()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_html_attribute(value: &str) -> String {
    escape_html(value)
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn multipart_boundary(bytes: &[u8]) -> String {
    format!("myopenpanels-{:x}", Sha256::digest(bytes))
}

fn multipart_body(boundary: &str, media: &WechatDraftMedia) -> Vec<u8> {
    let safe_name = media.file_name.replace(['\r', '\n', '"'], "_");
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"media\"; filename=\"{safe_name}\"\r\nContent-Type: {}\r\n\r\n",
        media.mime_type
    )
    .into_bytes();
    body.extend_from_slice(&media.bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}
