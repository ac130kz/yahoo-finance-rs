use chrono::serde::ts_seconds;
use chrono::{DateTime, Utc};
use reqwest::Url;
use serde::Deserialize;
use snafu::{ensure, OptionExt, ResultExt};
use std::env;

use crate::{ Interval, Result};
use crate::error::{self};

const BASE_URL: &str = "https://query1.finance.yahoo.com/v8/finance/chart/";

/// Helper function to build up the main query URL
fn build_query(symbol: &str) -> Result<Url> {
    let base = {
        let this = env::var("TEST_URL");
        let default = BASE_URL.to_string();
        match this {
            Ok(t) => t,
            // FIXME: ~const Drop doesn't quite work right yet
            #[allow(unused_variables)]
            Err(e) => default,
        }
    };
    Url::parse(&base)
        .context(error::InternalURLSnafu { url: &base })?
        .join(symbol)
        .context(error::InternalURLSnafu { url: symbol })
}

ez_serde!(Meta {
   symbol: String,

   #[serde(with = "ts_seconds")]
   first_trade_date: DateTime<Utc>,

   #[serde(rename = "regularMarketPrice")]
   current_price: f32,

   #[serde(rename = "chartPreviousClose")]
   previous_close: f32
});

ez_serde!(OHLCV {
   #[serde(rename = "open", default)]
   opens: Vec<Option<f64>>,

   #[serde(rename = "high", default)]
   highs: Vec<Option<f64>>,

   #[serde(rename = "low", default)]
   lows: Vec<Option<f64>>,

   #[serde(rename = "close", default)]
   closes: Vec<Option<f64>>,

   #[serde(rename = "volume", default)]
   volumes: Vec<Option<u64>>
});

ez_serde!(AdjustedClose {
    #[serde(rename = "adjclose", default)]
    adjusted_closes: Vec<Option<f64>>
});

ez_serde!(Indicators {
    #[serde(rename = "quote", default)]
    quotes: Vec<OHLCV>,

    #[serde(rename = "adjclose", default)]
    adjusted_closes: Vec<AdjustedClose>
});

ez_serde!(Data {
   meta: Meta,

   #[serde(rename = "timestamp", default)]
   timestamps: Vec<i64>,

   indicators: Indicators
});

ez_serde!(Error {
    code: String,
    description: String
});
ez_serde!(Chart { result: Option<Vec<Data>>, error: Option<Error> });
ez_serde!(Response { chart: Chart });

async fn load(url: &Url) -> Result<Data> {
    // make the call - we do not really expect this to fail.
    // ie - we won't 404 if the symbol doesn't exist
    let response = reqwest::get(url.clone())
        .await
        .context(error::RequestFailedSnafu)?;
    ensure!(
        response.status().is_success(),
        error::CallFailedSnafu {
            url: response.url().to_string(),
            status: response.status().as_u16()
        }
    );

    let data = response.text().await.context(error::UnexpectedErrorReadSnafu {
        url: url.to_string()
    })?;
    let chart = serde_json::from_str::<Response>(&data)
        .context(error::BadDataSnafu)?
        .chart;

    if chart.result.is_none() {
        // no result so we'd better have an error
        let err = chart.error.context(error::InternalLogicSnafu {
            reason: "error block exists without values",
        })?;
        error::ChartFailedSnafu {
            code: err.code,
            description: err.description,
        }
        .fail()?;
    }

    // we have a result to process
    let result = chart.result.context(error::UnexpectedErrorYahooSnafu)?;
    ensure!(!result.is_empty(), error::UnexpectedErrorYahooSnafu);
    Ok(result[0].clone())
}

pub async fn load_daily(symbol: &str, period: Interval) -> Result<Data> {
    let mut lookup = build_query(symbol)?;
    lookup
        .query_pairs_mut()
        .append_pair("range", &period.to_string())
        .append_pair("interval", "1d");

    load(&lookup).await
}

pub async fn load_daily_range(symbol: &str, start: i64, end: i64) -> Result<Data> {
    let mut lookup = build_query(symbol)?;
    lookup
        .query_pairs_mut()
        .append_pair("period1", &start.to_string())
        .append_pair("period2", &end.to_string())
        .append_pair("interval", "1d");

    load(&lookup).await
}
