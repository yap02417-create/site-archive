mod wallpapersclan;

use std::path::Path;

#[tokio::main]
async fn main() {
    println!("=== site-archive: wallpapersclan scraper ===\n");

    // output folder for downloaded wallpapers
    let output_dir = Path::new("wallpapersclan");
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir).expect("failed to create output dir");
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
                consecutive_errors = 0; // reset on success

                if items.is_empty() {
                    println!("no more items found! reached the end at page {}.", page);
                    break;
                }

                println!("found {} items on page {}", items.len(), page);
                let mut page_downloaded = 0;

                for item in &items {
                    let slug = &item.id;
                    let ext = if item.download_url.contains(".png") { "png" } else { "jpg" };
                    let filename = format!("{}.{}", slug, ext);
                    let filepath = output_dir.join(&filename);

                    // always write/update manifest
                    let manifest_path = output_dir.join(format!("{}.json", slug));
                    if let Ok(json) = serde_json::to_string_pretty(&item) {
                        let _ = std::fs::write(&manifest_path, json);
                    }

                    // skip if already downloaded
                    if filepath.exists() {
                        println!("  [skip] {} (already exists)", filename);
                        continue;
                    }

                    print!("  [dl] {} ... ", filename);

                    // retry downloads too
                    let mut dl_ok = false;
                    for dl_attempt in 1..=max_retries {
                        match wallpapersclan::download_wallpaper(&item.download_url, &filepath).await {
                            Ok(bytes) => {
                                println!("ok ({} KB)", bytes / 1024);
                                total_downloaded += 1;
                                page_downloaded += 1;
                                dl_ok = true;
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

                // update readme and progressively commit after every page with new downloads
                if page_downloaded > 0 {
                    generate_readme(output_dir);
                    
                    if std::env::var("GITHUB_ACTIONS").is_ok() {
                        println!("[ci] committing progress for page {}...", page);
                        let _ = std::process::Command::new("git").args(["add", "."]).status();
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
                
                // skip this page and keep going
                println!("skipping page {} and continuing...", page);
            }
        }
        
        page += 1;
        
        // small delay between pages to be polite and avoid rate limits
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    // final readme update to catch any stragglers
    generate_readme(output_dir);
    if std::env::var("GITHUB_ACTIONS").is_ok() {
        let _ = std::process::Command::new("git").args(["add", "."]).status();
        let _ = std::process::Command::new("git").args(["commit", "-m", "chore: final readme update [skip ci]"]).status();
        let _ = std::process::Command::new("git").args(["push"]).status();
    }

    println!("\n=== done! downloaded: {}, failed: {} ===", total_downloaded, total_failed);
}

fn generate_readme(output_dir: &Path) {
    let mut readme_content = String::from("# Wallpaper Archive\n\nAutomated archive of wallpapers to bypass Cloudflare and prevent dead links.\n\n## Gallery\n\n| Preview | Title | Tags |\n| --- | --- | --- |\n");
    
    if let Ok(entries) = std::fs::read_dir(output_dir) {
        let mut items = Vec::new();
        for entry in entries.flatten() {
            if entry.path().extension().map_or(false, |ext| ext == "json") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(item) = serde_json::from_str::<wallpapersclan::WallpaperEntry>(&content) {
                        items.push(item);
                    }
                }
            }
        }
        
        items.sort_by(|a, b| a.title.cmp(&b.title));
        
        for item in items {
            let ext = if item.download_url.contains(".png") { "png" } else { "jpg" };
            let img_path = format!("wallpapersclan/{}.{}", item.id, ext);
            let tags = item.tags.join(", ");
            readme_content.push_str(&format!("| <img src=\"{}\" width=\"200\"> | **{}**<br>[Download]({}) | {} |\n", 
                img_path, item.title, img_path, tags));
        }
    }
    
    let _ = std::fs::write("README.md", readme_content);
    println!("generated README.md with gallery index!");
}
