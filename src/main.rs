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

    // scrape pages 1-5 for a good collection
    let pages_to_scrape: Vec<u32> = (1..=5).collect();
    let limit_per_page = 30;

    let mut total_downloaded = 0u32;
    let mut total_failed = 0u32;

    for page in &pages_to_scrape {
        println!("\n--- page {} ---", page);

        match wallpapersclan::scrape_wallpapersclan(limit_per_page, *page).await {
            Ok(items) => {
                println!("found {} items on page {}", items.len(), page);

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
                            
                            // write manifest
                            let manifest_path = output_dir.join(format!("{}.json", slug));
                            if let Ok(json) = serde_json::to_string_pretty(&item) {
                                let _ = std::fs::write(manifest_path, json);
                            }
                        }
                        Err(e) => {
                            println!("FAILED: {}", e);
                            total_failed += 1;
                        }
                    }
                }
            }
            Err(e) => {
                println!("error scraping page {}: {}", page, e);
            }
        }
    }

    println!("\n=== done! downloaded: {}, failed: {} ===", total_downloaded, total_failed);

    // generate README.md index
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
        
        // sort by newest (we don't have dates, but we can sort by id or title, let's just sort by title)
        items.sort_by(|a, b| a.title.cmp(&b.title));
        
        for item in items {
            let ext = if item.download_url.contains(".png") { "png" } else { "jpg" };
            let img_path = format!("wallpapersclan/{}.{}", item.id, ext);
            let tags = item.tags.join(", ");
            readme_content.push_str(&format!("| <img src=\"{}\" width=\"200\"> | **{}**<br>[Download]({}) | {} |\n", 
                img_path, item.title, img_path, tags));
        }
    }
    
    std::fs::write("README.md", readme_content).expect("failed to write README.md");
    println!("generated README.md with gallery index!");
}
