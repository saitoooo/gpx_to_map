use anyhow::Result;
use image::{imageops, DynamicImage};
use std::{fs::File, io::BufWriter, io::Write, ops::Range, path::Path, path::PathBuf, sync::Arc, sync::Mutex, thread, time};

const JAPAN_MAP_URL: &str = "https://cyberjapandata.gsi.go.jp/xyz/std/";

pub struct MapBaseImage<'a> {
    max_store: usize,
    tile_dir: &'a str,
    cache: &'a Arc<Mutex<Vec<(i32, i32, DynamicImage)>>>,
}

impl<'a> MapBaseImage<'a> {
    pub fn new(tile_dir: &'a str, cache: &'a Arc<Mutex<Vec<(i32, i32, DynamicImage)>>>) -> Self {
        Self {
            max_store: 10,
            tile_dir,
            cache,
        }
    }

    pub fn use_tile_width(map_image_size: u32, tile_size: u32) -> u32 {
        (map_image_size - 1) / tile_size + 1
    }

    pub async fn get_tile_image(
        &mut self,
        map_image_size: u32,
        tile_size: u32,
        tile_x: i32,
        tile_y: i32,
        zoom: u32,
    ) -> Result<DynamicImage> {
        let tile_calc = Self::use_tile_width(map_image_size, tile_size);

        // 画像が一度生成されているか確認する
        let img = if let Some(img) = self.get_image(tile_x, tile_y) {
            img.clone()
        } else {
            // 取得するタイルの範囲を設定
            let x_range = Range {
                start: tile_x - tile_calc as i32,
                end: tile_x + tile_calc as i32 + 1,
            };

            let y_range = Range {
                start: tile_y - tile_calc as i32,
                end: tile_y + tile_calc as i32 + 1,
            };

            // 出力用の画像バッファ作成
            let mut img = DynamicImage::new_rgba8(
                (tile_calc * 2 + 1) * tile_size,
                (tile_calc * 2 + 1) * tile_size,
            );

            // タイルの合成
            for (x_pos, tile_x) in x_range.clone().enumerate() {
                for (y_pos, tile_y) in y_range.clone().enumerate() {
                    // タイル画像ダウンロード
                    Self::store_map_tile(self.tile_dir, zoom, tile_x, tile_y).await?;

                    // タイル画像合成していく
                    let tile_image =
                        image::open(Self::make_tile_filename(self.tile_dir, zoom, tile_x, tile_y))?;
                    let tile_image = tile_image.to_rgba();
                    imageops::overlay(
                        &mut img,
                        &tile_image,
                        x_pos as u32 * tile_size,
                        y_pos as u32 * tile_size,
                    );
                }
            }

            self.put_image(tile_x, tile_y, img.clone());

            img
        };

        Ok(img)
    }

    fn get_image(&mut self, tile_x: i32, tile_y: i32) -> Option<DynamicImage> {
        if let Some((_, _, img)) = self
            .cache.lock().unwrap()
            .iter()
            .find(|(tx, ty, _)| *tx == tile_x && *ty == tile_y)
        {
            Some(img.clone())
        } else {
            None
        }
    }

    fn put_image(&mut self, tile_x: i32, tile_y: i32, image: DynamicImage) {
        if self.get_image(tile_x, tile_y).is_none() {
            let mut cache = self.cache.lock().unwrap();
            cache.push((tile_x, tile_y, image.clone()));

            // 要素限界を超えたら古いところから消してきます
            while self.max_store < cache.len() {
                cache.remove(0);
            }
        }
    }

    fn make_tile_filename(target_dir: &str, zoom: u32, tile_x: i32, tile_y: i32) -> PathBuf {
        let store_file = Path::new(target_dir);
        let store_file = store_file.join(format!("{}-{}-{}.png", zoom, tile_x, tile_y));

        return store_file;
    }

    async fn store_map_tile(target_dir: &str, zoom: u32, tile_x: i32, tile_y: i32) -> Result<()> {
        // ファイル名生成
        let store_file = Self::make_tile_filename(target_dir, zoom, tile_x, tile_y);

        // ファイル存在チェック
        if store_file.exists() {
            return Ok(());
        }

        // URL 生成
        let url = format!("{}{}/{}/{}.png", JAPAN_MAP_URL, zoom, tile_x, tile_y);

        // HTTPでデータ取得
        let client = reqwest::Client::new();
        let response = client.get(&url).send().await?;

        // ファイルへ保存
        let f = File::create(store_file)?;
        let mut fw = BufWriter::new(f);

        fw.write_all(&response.bytes().await?)?;

        // アクセス終わったら一秒まつ(連続アクセスをしないようにするため)
        let wait_sec = time::Duration::from_secs(1);
        thread::sleep(wait_sec);

        Ok(())
    }
}
