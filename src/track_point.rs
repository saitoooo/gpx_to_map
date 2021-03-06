use chrono::{DateTime,  Duration,  Utc};
use gpx::Track;

pub struct GroupIterater<T:Iterator> {
    iterator: T,
    next_count: usize,
}

impl<T:Iterator> GroupIterater<T> {
    pub fn new(iterator: T, next_count: usize) -> Self {
        Self {
            iterator,
            next_count
        }
    }
}

impl<T> Iterator for GroupIterater<T> where T:Iterator {
    type Item = Vec<T::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut result: Vec<T::Item> = Vec::new();
        
        let mut now_count = 0;
        while let Some(item) = self.iterator.next() {
            
            result.push(item);

            now_count += 1;
            
            if now_count >= self.next_count {
                break;
            }
        }

        if now_count == 0 {
            None 
        } else {
            Some(result)
        }
    }
}



#[derive(Debug, Clone, Copy)]
pub struct TrackPoint {
    pub time: DateTime<Utc>,
    pub lat: f64,
    pub lng: f64,
}

pub struct TrackIter<'a> {
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
    pub fn get_iter(
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



#[test]
fn hoge() {
    use chrono::{ Datelike,  Timelike};
    use std::{fs::File, io::BufReader};

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
    use chrono::{ Datelike,  Timelike};
    use std::{fs::File, io::BufReader};


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
}
