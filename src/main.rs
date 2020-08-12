// https://qiita.com/MALORGIS/items/1a9114dd090e5b891bf7
// https://icon-rainbow.com/
// https://qiita.com/tasshi/items/de36d9add14f24317f47

use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDateTime, TimeZone, Timelike, Utc};
use globalmaptiles::GlobalMercator;
use gpx;
use gpx::Track;
use image::DynamicImage;
use std::fs;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::{
    ops::Range,
    path::{Path, PathBuf},
};
use std::{thread, time};

const OPENSTREAT_MAP_URL: &str = "https://tile.openstreetmap.org/";
const JAPAN_MAP_URL: &str = "https://cyberjapandata.gsi.go.jp/xyz/std/";

#[tokio::main]
async fn main() -> Result<()> {
    let zoom = 16;
    let map_image_size = 1000;
    let tile_dir = "tiles";
    let dest_dir = "dest";

    // ディレクトリ作成
    fs::create_dir_all(&tile_dir)?; //タイルディレクトリ
    fs::create_dir_all(&dest_dir)?; //出力ディレクトリ

    let f = File::open("sample_data\\大垂水峠かな.gpx")?;
    let reader = BufReader::new(f);

    let gpx = gpx::read(reader).map_err(|x| anyhow::anyhow!(x.description().to_string()))?;
    let track = gpx
        .tracks
        .first()
        .ok_or(anyhow::anyhow!("データがみつかりません"))?;

    let segment_data = get_points_every_second(track)?;
    for (pos, point) in segment_data.iter().enumerate() {
        let (tile_x, tile_y, pixel_x, pixel_y, pixel_size) =
            calc_tile_and_pixel(point.lat, point.lng, zoom);

        // 出力ファイル名を作成
        let dest_path = Path::new(dest_dir).join(format!("{}.png", pos));
        let dest_path = dest_path
            .to_str()
            .ok_or(anyhow::anyhow!("出力ファイル名生成に失敗"))?;

        let image = make_map_image(
            &tile_dir,
            zoom,
            tile_x,
            tile_y,
            pixel_x,
            pixel_y,
            pixel_size,
            map_image_size,
        )
        .await?;

        image.save_with_format(dest_path, image::ImageFormat::Png)?;
    }

    Ok(())
}

async fn make_map_image(
    tile_dir: &str,
    zoom: u32,
    tile_x: i32,
    tile_y: i32,
    pixel_x: i32,
    pixel_y: i32,
    tile_size: u32,
    map_image_size: u32,
) -> Result<DynamicImage> {
    // 必要なタイル数を計算
    let tile_calc = (map_image_size - 1) / tile_size + 1;

    println!("tile_calc: {}", tile_calc);

    // 取得するタイルの範囲を設定
    let x_range = Range {
        start: tile_x - tile_calc as i32,
        end: tile_x + tile_calc as i32 + 1,
    };

    let y_range = Range {
        start: tile_y - tile_calc as i32,
        end: tile_y + tile_calc as i32 + 1,
    };

    // タイル画像のダウンロード
    store_map_tile_range(tile_dir, zoom, &x_range, &y_range).await?;

    // 出力用の画像バッファ作成
    let mut img = DynamicImage::new_rgba8(
        (tile_calc * 2 + 1) * tile_size,
        (tile_calc * 2 + 1) * tile_size,
    );

    // タイルの合成
    for (x_pos, tile_x) in x_range.clone().enumerate() {
        for (y_pos, tile_y) in y_range.clone().enumerate() {
            let tile_image = image::open(make_tile_filename(tile_dir, zoom, tile_x, tile_y))?;
            let tile_image = tile_image.to_rgba();
            image::imageops::overlay(
                &mut img,
                &tile_image,
                x_pos as u32 * tile_size,
                y_pos as u32 * tile_size,
            );
        }
    }

    // タイルの切り抜き
    let crop_start_x = tile_calc * tile_size + pixel_x as u32 - map_image_size / 2;
    let crop_start_y = tile_calc * tile_size + pixel_y as u32 - map_image_size / 2;

    let dest_image = image::imageops::crop(
        &mut img,
        crop_start_x,
        crop_start_y,
        map_image_size,
        map_image_size,
    );
    let mut img = dest_image.to_image();

    // 自転車アイコン付与
    let cycle_img = image::open("assets/cycle.png")?.to_rgba();
    let cycle_img = image::imageops::resize(
        &cycle_img,
        map_image_size / 20,
        map_image_size / 20,
        image::imageops::FilterType::Triangle,
    );
    image::imageops::overlay(
        &mut img,
        &cycle_img,
        map_image_size / 2 - map_image_size / 40,
        map_image_size / 2 - map_image_size / 40,
    );

    let img = DynamicImage::ImageRgba8(img);
    Ok(img)
}

async fn store_map_tile_range(
    target_dir: &str,
    zoom: u32,
    tile_x_r: &Range<i32>,
    tile_y_r: &Range<i32>,
) -> Result<()> {
    for tile_x in tile_x_r.clone() {
        for tile_y in tile_y_r.clone() {
            store_map_tile(target_dir, zoom, tile_x, tile_y).await?
        }
    }

    Ok(())
}

fn make_tile_filename(target_dir: &str, zoom: u32, tile_x: i32, tile_y: i32) -> PathBuf {
    let store_file = Path::new(target_dir);
    let store_file = store_file.join(format!("{}-{}-{}.png", zoom, tile_x, tile_y));

    return store_file;
}

async fn store_map_tile(target_dir: &str, zoom: u32, tile_x: i32, tile_y: i32) -> Result<()> {
    // ファイル名生成
    let store_file = make_tile_filename(target_dir, zoom, tile_x, tile_y);

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

fn calc_tile_and_pixel(lat: f64, lng: f64, zoom: u32) -> (i32, i32, i32, i32, u32) {
    let t = GlobalMercator::default();
    let tile_size = t.tile_size() as f64;

    //タイル計算
    let (rx, ry) = t.lat_lon_to_meters(lat, lng);
    let (rx, ry) = t.meters_to_tile(rx, ry, zoom);
    let (tile_x, tile_y) = t.google_tile(rx, ry, zoom); // <- タイル位置

    // タイル内ピクセル
    let (a, b, c, d) = t.tile_lat_lon_bounds(rx, ry, zoom);
    let pixel_y = (tile_size - ((lat - a) * tile_size / (c - a)).floor()) as i32;
    let pixel_x = ((lng - b) * tile_size / (d - b)).floor() as i32;

    // 結果をタプルにして返します
    (tile_x, tile_y, pixel_x, pixel_y, t.tile_size())
}

#[derive(Debug)]
struct TrackPoint {
    time: DateTime<Utc>,
    lat: f64,
    lng: f64,
}

// 秒ごとの位置情報を取得
fn get_points_every_second(track: &Track) -> Result<Vec<TrackPoint>> {
    let mut results: Vec<TrackPoint> = Vec::new();
    let (start, end) = get_star_and_end_time(track)?;
    let start = start
        .with_nanosecond(0)
        .ok_or(anyhow::anyhow!("時間調整でエラーが発生しました"))?;
    let end = end
        .with_nanosecond(0)
        .ok_or(anyhow::anyhow!("時間調整でエラーが発生しました"))?;

    // 日時のあるポイントだけを取得します
    let mut points = track
        .segments
        .iter()
        .flat_map(|item| item.points.iter())
        .filter(|item| item.time.is_some())
        .peekable();

    let mut target = start.clone();
    let mut waypoint_opt = points.next();
    while target < end && waypoint_opt.is_some() {
        let point = waypoint_opt.unwrap();
        let peek_point_opt = points.peek();

        let point_time = point
            .time
            .unwrap()
            .with_nanosecond(0)
            .ok_or(anyhow::anyhow!("日付調整でエラーが発生しました"))?;

        // targetの日時が最後まで来ていたら抜ける
        if point_time < target && peek_point_opt.is_none() {
            println!("Breakで抜ける");
            break;
        }

        // peekな日時を取得
        let peek_time = peek_point_opt
            .unwrap()
            .time
            .unwrap()
            .with_nanosecond(0)
            .ok_or(anyhow::anyhow!("日付調整でエラーが発生しました"))?;

        // 次のデータの領域ならnextしてcontinue
        if point_time < peek_time && peek_time <= target {
            waypoint_opt = points.next();
            continue;
        }

        //
        if point_time == target {
            //todo!("現在データを保存");
            results.push(TrackPoint {
                time: point_time,
                lat: point.point().lat(),
                lng: point.point().lng(),
            })
        } else {
            //差分計算
            let diff_a: Duration = peek_time - point_time;
            let diff_b: Duration = target - point_time;

            let percent = (diff_b.num_seconds() as f64) / (diff_a.num_seconds() as f64);

            // lat, lng 計算
            let p1 = point.point();
            let p2 = peek_point_opt.unwrap().point();
            let lat = p1.lat() + (p2.lat() - p1.lat()) * percent;
            let lng = p1.lng() + (p2.lng() - p1.lng()) * percent;

            // データをストア
            results.push(TrackPoint {
                time: target,
                lat: lat,
                lng: lng,
            })
        }

        // 次の一秒分の処理
        target = target + Duration::seconds(1);
    }

    // Err(anyhow::anyhow!("test"))

    Ok(results)
}

fn get_star_and_end_time(track: &Track) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let (min, max) = track
        .segments
        .iter()
        .fold((None, None), |(min, max), segment| {
            let (min, max) = segment.points.iter().fold((min, max), |(min, max), point| {
                if let Some(dt) = point.time {
                    //最小チェック
                    let next_min = if min.is_none() || dt < min.unwrap() {
                        Some(dt)
                    } else {
                        min
                    };

                    //最大チェック
                    let next_max = if max.is_none() || dt > max.unwrap() {
                        Some(dt)
                    } else {
                        max
                    };

                    (next_min, next_max)
                } else {
                    (min, max)
                }
            });

            (min, max)
        });

    if min.is_none() || max.is_none() {
        Err(anyhow::anyhow!("データが見つかりませんでした"))
    } else {
        Ok((min.unwrap(), max.unwrap()))
    }
}
