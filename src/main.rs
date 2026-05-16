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
    let mut page = 1;

    loop {
        println!("\n--- page {} ---", page);

        match wallpapersclan::scrape_wallpapersclan(12, page).await {
            Ok(items) => {
                if items.is_empty() {
                    println!("no more items found! reached the end at page {}.", page);
                    break;
                }

                println!("found {} items on page {}", items.len(), page);
                let mut page_downloaded = 0;

                for item in &items {
                    let slug = &item.id;
                    let ext = if item.download_url.contains(".png") {
                        "png"
                    } else {
                        "jpg"
                    };
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

                    match wallpapersclan::download_wallpaper(&item.download_url, &filepath).await {
                        Ok(bytes) => {
                            println!("ok ({} KB)", bytes / 1024);
                            total_downloaded += 1;
                            page_downloaded += 1;
                        }
                        Err(e) => {
                            println!("FAILED: {}", e);
                            total_failed += 1;
                        }
                    }
                }
                // If we downloaded new things, update the README and push progressively to be failsafe
                if page_downloaded > 0 {
                    generate_readme(output_dir);
                    
                    // Progressive commit in GitHub Actions to avoid timeout data loss
                    if std::env::var("GITHUB_ACTIONS").is_ok() {
                        println!("[ci] committing progress for page {}...", page);
                        let _ = std::process::Command::new("git").args(["add", "."]).status();
                        let _ = std::process::Command::new("git").args(["commit", "-m", &format!("chore: archive page {} [skip ci]", page)]).status();
                        let _ = std::process::Command::new("git").args(["push"]).status();
                    }
                }
            }
            Err(e) => {
                println!("error scraping page {}: {}", page, e);
                println!("halting due to error.");
                break;
            }
        }
        
        page += 1;
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
