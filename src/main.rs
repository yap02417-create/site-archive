mod wallpapersclan;

use std::collections::HashSet;
use std::path::Path;

const CDN_BASE: &str = "https://raw.githubusercontent.com/yap02417-create/site-archive/main/wallpapersclan";

#[tokio::main]
async fn main() {
    println!("=== site-archive: wallpapersclan scraper ===\n");

    // output folder for downloaded wallpapers
    let output_dir = Path::new("wallpapersclan");
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir).expect("failed to create output dir");
    }

    // parse existing readme to get already-downloaded ids (avoids needing the actual image files)
    let mut existing_ids = load_existing_ids();
    println!("found {} already-archived wallpapers in README.md", existing_ids.len());

    // ensure readme exists with header if it doesn't
    if !Path::new("README.md").exists() {
        let header = "# Wallpaper Archive\n\nAutomated archive of wallpapers to bypass Cloudflare and prevent dead links.\n\n## Gallery\n\n| Preview | Title | Tags |\n| --- | --- | --- |\n";
        let _ = std::fs::write("README.md", header);
    }

    let mut total_downloaded = 0u32;
    let mut total_failed = 0u32;
    let mut page = 1u32;
    let mut consecutive_errors = 0u32;
    let max_retries = 3u32;

    loop {
        println!("\n--- page {} ---", page);

        let mut attempt = 0;
        let result = loop {
            attempt += 1;
            match wallpapersclan::scrape_wallpapersclan(12, page).await {
                Ok(items) => break Ok(items),
                Err(e) => {
                    if attempt >= max_retries {
                        break Err(e);
                    }
                    let wait = attempt * 5;
                    println!("[retry] page {} attempt {}/{} failed: {} — waiting {}s...", page, attempt, max_retries, e, wait);
                    tokio::time::sleep(std::time::Duration::from_secs(wait as u64)).await;
                }
            }
        };

        match result {
            Ok(items) => {
                consecutive_errors = 0;

                if items.is_empty() {
                    println!("no more items found! reached the end at page {}.", page);
                    break;
                }

                println!("found {} items on page {}", items.len(), page);
                let mut page_downloaded = 0;
                let mut new_readme_rows = String::new();

                for item in &items {
                    let slug = &item.id;

                    // skip if already in readme (already archived)
                    if existing_ids.contains(slug.as_str()) {
                        println!("  [skip] {} (already archived)", slug);
                        continue;
                    }

                    let ext = if item.download_url.contains(".png") { "png" } else { "jpg" };
                    let filename = format!("{}.{}", slug, ext);
                    let filepath = output_dir.join(&filename);

                    // always write/update manifest
                    let manifest_path = output_dir.join(format!("{}.json", slug));
                    if let Ok(json) = serde_json::to_string_pretty(&item) {
                        let _ = std::fs::write(&manifest_path, json);
                    }

                    print!("  [dl] {} ... ", filename);

                    // retry downloads
                    for dl_attempt in 1..=max_retries {
                        match wallpapersclan::download_wallpaper(&item.download_url, &filepath).await {
                            Ok(bytes) => {
                                println!("ok ({} KB)", bytes / 1024);
                                total_downloaded += 1;
                                page_downloaded += 1;

                                // build readme row and track this id
                                let cdn_url = format!("{}/{}.{}", CDN_BASE, slug, ext);
                                let tags = item.tags.join(", ");
                                new_readme_rows.push_str(&format!(
                                    "| <img src=\"{}\" width=\"200\"> | **{}**<br>[Download]({}) | {} |\n",
                                    cdn_url, item.title, cdn_url, tags
                                ));
                                existing_ids.insert(slug.clone());
                                break;
                            }
                            Err(e) => {
                                if dl_attempt < max_retries {
                                    print!("retry {}... ", dl_attempt + 1);
                                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                } else {
                                    println!("FAILED after {} attempts: {}", max_retries, e);
                                    total_failed += 1;
                                }
                            }
                        }
                    }
                }

                // append new rows to readme and commit
                if page_downloaded > 0 {
                    append_to_readme(&new_readme_rows);

                    if std::env::var("GITHUB_ACTIONS").is_ok() {
                        println!("[ci] committing progress for page {}...", page);
                        let _ = std::process::Command::new("git").args(["add", "--sparse", "README.md", "wallpapersclan"]).status();
                        let _ = std::process::Command::new("git")
                            .args(["commit", "-m", &format!("chore: archive page {} ({} new) [skip ci]", page, page_downloaded)])
                            .status();
                        let _ = std::process::Command::new("git").args(["push"]).status();
                    }
                }
            }
            Err(e) => {
                consecutive_errors += 1;
                println!("error scraping page {} after {} retries: {}", page, max_retries, e);

                if consecutive_errors >= 5 {
                    println!("too many consecutive failures ({}), halting.", consecutive_errors);
                    break;
                }

                println!("skipping page {} and continuing...", page);
            }
        }

        page += 1;

        // small delay between pages to be polite
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    // final sort to keep readme alphabetical (matches old behavior)
    sort_readme();
    if std::env::var("GITHUB_ACTIONS").is_ok() {
        let _ = std::process::Command::new("git").args(["add", "--sparse", "README.md", "wallpapersclan"]).status();
        let _ = std::process::Command::new("git").args(["commit", "-m", "chore: sort readme alphabetically [skip ci]"]).status();
        let _ = std::process::Command::new("git").args(["push"]).status();
    }

    println!("\n=== done! downloaded: {}, failed: {} ===", total_downloaded, total_failed);
}

/// parse readme.md to extract all wallpaper ids that are already archived
fn load_existing_ids() -> HashSet<String> {
    let mut ids = HashSet::new();
    if let Ok(content) = std::fs::read_to_string("README.md") {
        for line in content.lines() {
            // each row has: /wallpapersclan/SLUG.ext in the cdn url
            if let Some(start) = line.find("/wallpapersclan/") {
                let after = &line[start + 16..]; // skip "/wallpapersclan/"
                if let Some(dot) = after.find('.') {
                    let slug = &after[..dot];
                    if !slug.is_empty() {
                        ids.insert(slug.to_string());
                    }
                }
            }
        }
    }
    ids
}

/// append new rows to the end of readme.md
fn append_to_readme(rows: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;

    if let Ok(mut file) = OpenOptions::new().append(true).open("README.md") {
        let _ = file.write_all(rows.as_bytes());
        println!("appended {} new entries to README.md", rows.lines().count());
    }
}

/// sort the readme table rows alphabetically by title (keeps output identical to old regenerate behavior)
fn sort_readme() {
    let content = match std::fs::read_to_string("README.md") {
        Ok(c) => c,
        Err(_) => return,
    };

    let lines: Vec<&str> = content.lines().collect();

    // header is everything before the first table data row (lines starting with "| <img")
    let mut header_lines = Vec::new();
    let mut data_rows = Vec::new();

    for line in &lines {
        if line.starts_with("| <img") {
            data_rows.push(*line);
        } else {
            if data_rows.is_empty() {
                header_lines.push(*line);
            }
        }
    }

    // sort rows alphabetically (the title is the second column, but since the slug is in the url
    // and entries have similar structure, sorting the whole line works the same)
    data_rows.sort();

    let mut output = header_lines.join("\n");
    output.push('\n');
    for row in &data_rows {
        output.push_str(row);
        output.push('\n');
    }

    let _ = std::fs::write("README.md", output);
    println!("sorted readme: {} entries alphabetically", data_rows.len());
}
