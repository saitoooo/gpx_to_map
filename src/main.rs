// https://qiita.com/MALORGIS/items/1a9114dd090e5b891bf7
// https://icon-rainbow.com/
// https://qiita.com/tasshi/items/de36d9add14f24317f47

mod arguments;

use anyhow::Result;
use arguments::Opts;
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use clap::Clap;
use globalmaptiles::GlobalMercator;
use gpx::Track;
use image::{imageops, DynamicImage};
use std::{
    fs,
    fs::File,
    io::{BufReader, BufWriter, Write},
    ops::Range,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread, time,
};

// const OPENSTREAT_MAP_URL: &str = "https://tile.openstreetmap.org/";
const JAPAN_MAP_URL: &str = "https://cyberjapandata.gsi.go.jp/xyz/std/";
const ASSET_CYCLE_ICON: &str = "assets/cycle.png";

#[tokio::main]
async fn main() -> Result<()> {
    let opts: Opts = Opts::parse();

    gpx_to_map_movie(
        &opts.gpx_file,
        &opts.dest_file,
        opts.get_start_date(),
        opts.get_end_date(),
        opts.map_image_size,
        opts.zoom,
        &opts.tile_dir,
    )
    .await
}

#[test]
fn hoge() {
    let f = File::open("sample_data/大垂水峠かな.gpx").unwrap();
    let reader = BufReader::new(f);

    let gpx = gpx::read(reader).unwrap();
    let track = gpx.tracks.first().unwrap();

    // 2020-07-31T22:27:46.000Z

    let start_date: Option<DateTime<Utc>> = Utc::now()
        .with_year(2020)
        .and_then(|t| t.with_month(7))
        .and_then(|t| t.with_day(31))
        .and_then(|t| t.with_hour(22))
        .and_then(|t| t.with_minute(27))
        .and_then(|t| t.with_second(46))
        .and_then(|t| t.with_nanosecond(0));

    let end_date: Option<DateTime<Utc>> = None;

    let mut iter = TrackIter::get_iter(track, 60, start_date, end_date);
    let r = iter.move_to_dt(start_date.unwrap());
    let next = iter.point_next.clone().unwrap();
    let prev = iter.point_prev.clone().unwrap();

    assert_eq!(r, true);
    let r2 = iter.move_to_dt(start_date.unwrap());
    let next2 = iter.point_next.clone().unwrap();
    let prev2 = iter.point_prev.clone().unwrap();

    assert_eq!(r2, true);

    assert_eq!(prev.time, prev2.time);
    assert_eq!(next.time, next2.time);

    println!("{:?} {:?}", prev, next);

    println!("{:?} {:?}", prev2, next2);

    let r = TrackIter::calc_position(&prev, &next, start_date.unwrap());
    println!("{:?}", r);

    assert_eq!(next.lat, r.lat);
    assert_eq!(next.lng, r.lng);
}

#[test]
fn hage() {
    let f = File::open("sample_data/大垂水峠かな.gpx").unwrap();
    let reader = BufReader::new(f);

    let gpx = gpx::read(reader).unwrap();
    let track = gpx.tracks.first().unwrap();

    let start_date: Option<DateTime<Utc>> = Utc::now()
        .with_year(2020)
        .and_then(|t| t.with_month(7))
        .and_then(|t| t.with_day(31))
        .and_then(|t| t.with_hour(22))
        .and_then(|t| t.with_minute(27))
        .and_then(|t| t.with_second(46))
        .and_then(|t| t.with_nanosecond(0));


        let end_date: Option<DateTime<Utc>> = Utc::now()
        .with_year(2020)
        .and_then(|t| t.with_month(7))
        .and_then(|t| t.with_day(31))
        .and_then(|t| t.with_hour(22))
        .and_then(|t| t.with_minute(30))
        .and_then(|t| t.with_second(46))
        .and_then(|t| t.with_nanosecond(0));

    let iter = TrackIter::get_iter(track, 2, start_date, end_date);
    for track in iter {
        println!("{:?}", track);
    }
    panic!("kkk");

}

struct TrackIter<'a> {
    points: Box<(dyn Iterator<Item = TrackPoint> + 'a)>,
    fps: usize,
    start_dt: Option<DateTime<Utc>>,

    end_dt: Option<DateTime<Utc>>,

    current: Option<DateTime<Utc>>,
    current_fps: usize,
    point_prev: Option<TrackPoint>,
    point_next: Option<TrackPoint>,
}

impl<'a> TrackIter<'a> {
    fn get_iter(
        track: &'a Track,
        fps: usize,
        start_dt: Option<DateTime<Utc>>,
        end_dt: Option<DateTime<Utc>>,
    ) -> Self {
        // 日時のあるポイントだけを取得します
        let points = track
            .segments
            .iter()
            .flat_map(|item| {
                let xx = item.clone();
                xx.points.into_iter().clone()
            })
            .filter(|item| item.time.is_some())
            .map(|point| TrackPoint {
                time: point.time.unwrap(),
                lat: point.point().lat(),
                lng: point.point().lng(),
            });

        let iter = points.into_iter();
        TrackIter {
            points: Box::new(iter),
            fps,
            start_dt,
            end_dt,
            current: None,
            current_fps: 0,
            point_next: None,
            point_prev: None,
        }
    }

    // 指定された日付の位置まで移動する関数
    fn move_to_dt(&mut self, dt: DateTime<Utc>) -> bool {
        let f = |dt: DateTime<Utc>, point: &Option<TrackPoint>| -> bool {
            // 最初に範囲内であればそのまま抜ける
            if let Some(tp) = point {
                if tp.time >= dt {
                    return true; //場所見つかった
                }
            }

            false
        };

        if f(dt, &self.point_next) {
            return true;
        }

        // 指定された日付までデータを探す
        while let Some(point) = self.points.next() {
            self.point_prev = self.point_next;
            self.point_next = Some(point);

            // 開始日付チェック
            if f(dt, &self.point_next) {
                return true; //場所見つかった
            }
        }

        false
    }

    // 位置計算
    fn calc_position(prev: &TrackPoint, next: &TrackPoint, current: DateTime<Utc>) -> TrackPoint {
        let current_mills = current.timestamp_millis();
        let prev_mills = prev.time.timestamp_millis();
        let next_mills = next.time.timestamp_millis();

        // current_millsが範囲内かチェック
        if prev_mills > current_mills && next_mills < current_mills {
            panic!("パラメータ範囲エラー");
        }

        // prev, next が同一の場合、計算不要でprevを返す(先頭データのみ発生する)
        if prev_mills == next_mills {
            return prev.clone();
        }

        // 比率から lat, lng を計算
        let ratio =
            (current_mills as f64 - prev_mills as f64) / (next_mills as f64 - prev_mills as f64);

        TrackPoint {
            lat: prev.lat + (next.lat - prev.lat) * ratio,
            lng: prev.lng + (next.lng - prev.lng) * ratio,
            time: current,
        }
    }
}

impl<'a> Iterator for TrackIter<'a> {
    type Item = TrackPoint;

    fn next(&mut self) -> Option<Self::Item> {
        // 初回かどうかの確認
        if self.current.is_none() {
            // 最初の日付を取ります
            if let Some(tp) = self.points.next() {
                self.point_prev = Some(tp);
                self.point_next = Some(tp);

                self.current = if self.start_dt.is_some() {
                    self.start_dt
                } else {
                    Some(tp.time)
                };

                self.current_fps = 0;
            } else {
                return None;
            }
        }

        // ターゲットの時間をmsec単位で取得する
        let duration = Duration::milliseconds((self.current_fps * 100 / self.fps) as i64);
        let current: DateTime<Utc> = self.current.unwrap() + duration;

        // 終了時間過ぎているかチェック
        if let Some(dt) = self.end_dt {
            if dt < current {
                return None
            }
        }

        // データを探します
        if self.move_to_dt(current) == false {
            return None;
        }

        // 位置計算
        let track_point = TrackIter::calc_position(
            &self.point_prev.unwrap(),
            &self.point_next.unwrap(),
            current,
        );

        // 次のデータへカウントアップ
        self.current_fps = self.current_fps + 1;
        if self.current_fps >= self.fps {
            self.current = Some(self.current.unwrap() + Duration::seconds(1));
            self.current_fps = 0;
        }

        Some(track_point)
    }
}

async fn gpx_to_map_movie(
    gpx_file: &str,
    dest_path: &str,
    start_date: Option<DateTime<Utc>>,
    end_date: Option<DateTime<Utc>>,
    map_image_size: u32,
    zoom: u32,
    tile_dir: &str,
) -> Result<()> {
    let f = File::open(gpx_file)?;
    let reader = BufReader::new(f);

    let gpx = gpx::read(reader).map_err(|x| anyhow::anyhow!(x.description().to_string()))?;
    let track = gpx
        .tracks
        .first()
        .ok_or(anyhow::anyhow!("データがみつかりません"))?;

    let iter = TrackIter::get_iter(track, 60, start_date, end_date);


    let mut process = make_ffmpeg_process(map_image_size, dest_path)?;
    let stdin = process.stdin.as_mut().unwrap();

    // ディレクトリ作成
    fs::create_dir_all(&tile_dir)?; //タイルディレクトリ

    for point in iter {
        let (tile_x, tile_y, pixel_x, pixel_y, pixel_size) =
            calc_tile_and_pixel(point.lat, point.lng, zoom);

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
        .await;

        let image = match image {
            Ok(i) => i,
            Err(e) => {
                println!("{}", e);
                process.wait()?;
                std::process::exit(-1);
            }
        };

        let r = image.as_rgba8().unwrap();
        let r = r.clone().into_raw();
        stdin.write_all(&r).unwrap();
    }

    process.wait()?;

    Ok(())
}

fn make_ffmpeg_process(image_size: u32, outfile: &str) -> Result<Child> {
    let cmd = "ffmpeg";
    let size_text = format!("{}x{}", image_size, image_size);
    let mut cmd = Command::new(cmd);
    let cmd = cmd
        .args(&[
            "-framerate",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &size_text,
            "-i",
            "-",
            "-pix_fmt",
            "yuv420p",
            "-vcodec",
            "libx264",
            "-movflags",
            "faststart",
            "-r",
            "60",
            outfile,
        ])
        .stdin(Stdio::piped());
    Ok(cmd.spawn()?)
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
            imageops::overlay(
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

    let dest_image = imageops::crop(
        &mut img,
        crop_start_x,
        crop_start_y,
        map_image_size,
        map_image_size,
    );
    let mut img = dest_image.to_image();

    // 自転車アイコン付与
    let mut icon_path = std::env::current_exe()?
        .parent()
        .map(|p| p.join(ASSET_CYCLE_ICON))
        .ok_or(anyhow::anyhow!("cycle.pngのパスが解決できませんでした"))?;
    if icon_path.exists() == false {
        icon_path = std::env::current_dir()?.join(ASSET_CYCLE_ICON);
    }

    let cycle_img = image::open(icon_path)?.to_rgba();
    let cycle_img = imageops::resize(
        &cycle_img,
        map_image_size / 20,
        map_image_size / 20,
        imageops::FilterType::Triangle,
    );
    imageops::overlay(
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

#[derive(Debug, Clone, Copy)]
struct TrackPoint {
    time: DateTime<Utc>,
    lat: f64,
    lng: f64,
}


