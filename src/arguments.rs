use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use clap::Clap;

#[derive(Clap)]
#[clap(version = "0.1", author = "Yoshiyuki Saito")]
pub struct Opts {
    #[clap(about = "処理対象のgpxファイル")]
    pub gpx_file: String,

    #[clap(about = "出力するmp4ファイル", default_value = "dest.mp4")]
    pub dest_file: String,

    #[clap(short, long, about = "処理対象日時（開始）- %Y-%m-%d %H:%M:%S")]
    pub start_dt: Option<String>,

    #[clap(short, long, about = "処理対象日時（終了）- %Y-%m-%d %H:%M:%S")]
    pub end_dt: Option<String>,

    #[clap(short, long, about = "動画の一辺の長さ", default_value = "400")]
    pub map_image_size: u32,

    #[clap(
        short,
        long,
        about = "マップタイルのズームレベル",
        default_value = "16"
    )]
    pub zoom: u32,

    #[clap(short, long, about = "マップタイル保存ディレクトリ", default_value = "tiles")]
    pub tile_dir: String,
}

impl Opts {
    pub fn get_start_date(&self) -> Option<DateTime<Utc>> {
        get_date_parameter(&self.start_dt)
    }

    pub fn get_end_date(&self) -> Option<DateTime<Utc>> {
        get_date_parameter(&self.end_dt)
    }
}

fn get_date_parameter(date_str: &Option<String>) -> Option<DateTime<Utc>> {
    if date_str.is_none() {
        return None;
    }

    let dt = date_str.clone().unwrap();
    let dt = NaiveDateTime::parse_from_str(&dt, "%Y-%m-%d %H:%M:%S")
        .expect("日付パラメータが正しいフォーマットではありません");
    let dt: DateTime<Utc> = Local.from_local_datetime(&dt).unwrap().into();

    Some(dt)
}
