// https://qiita.com/MALORGIS/items/1a9114dd090e5b891bf7
// https://icon-rainbow.com/
// https://qiita.com/tasshi/items/de36d9add14f24317f47

mod arguments;
mod map_image;
mod track_point;

use anyhow::Result;
use arguments::Opts;
use chrono::{DateTime, Utc};
use clap::Clap;
use globalmaptiles::GlobalMercator;
use image::{imageops, DynamicImage};
use map_image::MapBaseImage;
use std::{fs, fs::File, io::{BufReader, Write}, process::{Child, Command, Stdio}, sync::Arc, sync::Mutex, sync::mpsc::{self, Receiver, Sender}, thread};
use track_point::{GroupIterater, TrackIter};
use tokio::{task::JoinHandle};
// const OPENSTREAT_MAP_URL: &str = "https://tile.openstreetmap.org/";

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

    // let giter = GroupIterater::new(TrackIter::get_iter(track, 30, start_date, end_date), 24);
    let iter = TrackIter::get_iter(track, 30, start_date, end_date);

    // ディレクトリ作成
    fs::create_dir_all(&tile_dir)?; //タイルディレクトリ

    // 通信用チャンネル作成
    let (tx, rx):(Sender<Mutex<Option<DynamicImage>>>, Receiver<Mutex<Option<DynamicImage>>>) = mpsc::channel();

    // タイルのキャッシュを取得
    let tile_cache:Arc<Mutex<Vec<(i32, i32, DynamicImage)>>> = Arc::new(Mutex::new(Vec::new()));

    // 出力用スレッド生成
    let dest_path = dest_path.to_string();
    let handle = thread::spawn( move || {
        let mut process = make_ffmpeg_process(map_image_size, &dest_path).unwrap();
        let stdin = process.stdin.as_mut().unwrap();

        while let Ok(x) = rx.recv() {
            let r = x.lock().unwrap().clone();
            

            if let Some(data) = r {
                let r = data.as_rgba8().unwrap();
                let r = r.clone().into_raw();

                stdin.write_all(&r).unwrap();
            } else {
                break;
            }

        }
        process.wait().unwrap();

    });
    
    let giter = GroupIterater::new(iter, 6);


    //for point in iter {
    for group_items in giter {
        let mut tasks :Vec<Box<JoinHandle<Result<DynamicImage>>>> = Vec::new();

        for point in group_items {

            let (tile_x, tile_y, pixel_x, pixel_y, pixel_size) =
                calc_tile_and_pixel(point.lat, point.lng, zoom);
    
            let future = make_map_image(
                zoom,
                tile_x,
                tile_y,
                pixel_x,
                pixel_y,
                pixel_size,
                map_image_size,
                tile_dir.to_string(),
                tile_cache.clone(),
            );
            
            let x = tokio::task::spawn(future);
            tasks.push(Box::new(x));
        }

        for task in tasks {
           let image = task.await?;
           match image {
               Ok(image) => {
                    tx.send(Mutex::new(Some( image ))).unwrap();
               },
               Err(i) => println!("{}", i)
           }

            

        }
    };  

    tx.send(Mutex::new(None)).unwrap();

    handle.join().expect("出力処理でエラーが発生しました");


    Ok(())
}

fn make_ffmpeg_process(image_size: u32, outfile: &str) -> Result<Child> {
    let cmd = "ffmpeg";
    let size_text = format!("{}x{}", image_size, image_size);
    let mut cmd = Command::new(cmd);
    let cmd = cmd
        .args(&[
            "-framerate",
            "30",
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

async fn make_map_image<'a>(
    zoom: u32,
    tile_x: i32,
    tile_y: i32,
    pixel_x: i32,
    pixel_y: i32,
    tile_size: u32,
    map_image_size: u32,
    tile_dir: String,
    tile_cache: Arc<Mutex<Vec<(i32, i32, DynamicImage)>>>,
) -> Result<DynamicImage> {

    let mut image_store = MapBaseImage::new(&tile_dir, &tile_cache);
        
    // 必要なタイル数を計算
    let tile_calc = (map_image_size - 1) / tile_size + 1;

    // 画像が一度生成されているか確認する
    let mut img = image_store
        .get_tile_image(map_image_size, tile_size, tile_x, tile_y, zoom)
        .await?;

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
