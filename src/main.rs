// https://qiita.com/MALORGIS/items/1a9114dd090e5b891bf7

use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDateTime, TimeZone, Timelike, Utc};
use globalmaptiles::GlobalMercator;
use gpx;
use gpx::Track;
use std::fs::File;
use std::io::BufReader;

fn main() -> Result<()> {
    let lat = 35.6157235;
    let lng = 139.152164;
    let zoom = 16;

    let (tile_x, tile_y, pixel_x, pixel_y) = calc_tile_and_pixel(lat, lng, zoom);

    println!(
        "https://tile.openstreetmap.org/{}/{}/{}.png",
        zoom, tile_x, tile_y
    );
    println!(
        "https://cyberjapandata.gsi.go.jp/xyz/std/{}/{}/{}.png",
        zoom, tile_x, tile_y
    );

    println!("pos: {}-{}", pixel_x, pixel_y);

    //gps_test()?;
    Ok(())
}

fn calc_tile_and_pixel(lat: f64, lng: f64, zoom: u32) -> (i32, i32, i32, i32) {
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
    (tile_x, tile_y, pixel_x, pixel_y)
}

fn gps_test() -> Result<()> {
    let xxx = NaiveDateTime::parse_from_str("2020-08-01 11:11:11", "%Y-%m-%d %H:%M:%S")?;
    let yyy: DateTime<Utc> = Local.from_local_datetime(&xxx).unwrap().into();
    println!("{}", xxx);
    println!("{}", yyy);

    println!("Hello, world!");

    let f = File::open("sample_data\\大垂水峠かな.gpx")?;
    let reader = BufReader::new(f);

    let gpx_result = gpx::read(reader);
    let o = gpx_result.map(|gpx| {
        println!("トラック数: {}", gpx.tracks.len());

        for (pos, track) in gpx.tracks.iter().enumerate() {
            let (min, max) = get_star_and_end_time(track).unwrap();
            println!("min: {}, max: {}", min, max);
            let segment_data = get_points_every_second(track).unwrap();
            for x in segment_data {
                println!("{:?}", x);
            }

            println!("セグメント数 {}: {}", pos, track.segments.len());

            // for (segment_no, segment) in track.segments.iter().enumerate() {
            //     for (point_no, waypoint) in segment.points.iter().enumerate() {
            //         let point = waypoint.point();
            //         println!(
            //             "{}-{}: time={}, lat={}, lng={}",
            //             segment_no,
            //             point_no,
            //             waypoint.time.unwrap(),
            //             point.lat(),
            //             point.lng()
            //         );
            //     }
            // }
        }

        gpx
    });

    o.map_err(|x| anyhow::anyhow!(x.description().to_string()))?;

    Ok(())
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
