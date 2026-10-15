use serde::Serialize;
use sqlx::MySqlPool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[derive(Default)]
pub struct MetricsCollector {
    // Acquire metrics
    pub acquire_count: AtomicU64,
    pub acquire_total_micros: AtomicU64,
    pub acquire_max_micros: AtomicU64,
    pub acquire_lt_1ms: AtomicU64,
    pub acquire_1_to_5ms: AtomicU64,
    pub acquire_5_to_10ms: AtomicU64,
    pub acquire_10_to_25ms: AtomicU64,
    pub acquire_25_to_50ms: AtomicU64,
    pub acquire_50_to_100ms: AtomicU64,
    pub acquire_gte_100ms: AtomicU64,

    // Query metrics
    pub query_count: AtomicU64,
    pub query_total_micros: AtomicU64,
    pub query_max_micros: AtomicU64,
    pub query_lt_1ms: AtomicU64,
    pub query_1_to_5ms: AtomicU64,
    pub query_5_to_10ms: AtomicU64,
    pub query_10_to_25ms: AtomicU64,
    pub query_gte_25ms: AtomicU64,
}

pub static DB_METRICS: MetricsCollector = MetricsCollector {
    acquire_count: AtomicU64::new(0),
    acquire_total_micros: AtomicU64::new(0),
    acquire_max_micros: AtomicU64::new(0),
    acquire_lt_1ms: AtomicU64::new(0),
    acquire_1_to_5ms: AtomicU64::new(0),
    acquire_5_to_10ms: AtomicU64::new(0),
    acquire_10_to_25ms: AtomicU64::new(0),
    acquire_25_to_50ms: AtomicU64::new(0),
    acquire_50_to_100ms: AtomicU64::new(0),
    acquire_gte_100ms: AtomicU64::new(0),

    query_count: AtomicU64::new(0),
    query_total_micros: AtomicU64::new(0),
    query_max_micros: AtomicU64::new(0),
    query_lt_1ms: AtomicU64::new(0),
    query_1_to_5ms: AtomicU64::new(0),
    query_5_to_10ms: AtomicU64::new(0),
    query_10_to_25ms: AtomicU64::new(0),
    query_gte_25ms: AtomicU64::new(0),
};

pub static CONFIGURED_POOL_MAX: AtomicU64 = AtomicU64::new(10);

pub fn set_configured_pool_max(max: u32) {
    CONFIGURED_POOL_MAX.store(max as u64, Ordering::Relaxed);
}

pub fn get_configured_pool_max() -> u32 {
    CONFIGURED_POOL_MAX.load(Ordering::Relaxed) as u32
}

pub fn record_pool_acquire(duration: Duration) {
    let micros = duration.as_micros() as u64;
    DB_METRICS.acquire_count.fetch_add(1, Ordering::Relaxed);
    DB_METRICS
        .acquire_total_micros
        .fetch_add(micros, Ordering::Relaxed);
    DB_METRICS
        .acquire_max_micros
        .fetch_max(micros, Ordering::Relaxed);

    let millis = duration.as_millis();
    if millis < 1 {
        DB_METRICS.acquire_lt_1ms.fetch_add(1, Ordering::Relaxed);
    } else if millis < 5 {
        DB_METRICS.acquire_1_to_5ms.fetch_add(1, Ordering::Relaxed);
    } else if millis < 10 {
        DB_METRICS.acquire_5_to_10ms.fetch_add(1, Ordering::Relaxed);
    } else if millis < 25 {
        DB_METRICS
            .acquire_10_to_25ms
            .fetch_add(1, Ordering::Relaxed);
    } else if millis < 50 {
        DB_METRICS
            .acquire_25_to_50ms
            .fetch_add(1, Ordering::Relaxed);
    } else if millis < 100 {
        DB_METRICS
            .acquire_50_to_100ms
            .fetch_add(1, Ordering::Relaxed);
    } else {
        DB_METRICS.acquire_gte_100ms.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn record_query_duration(duration: Duration) {
    let micros = duration.as_micros() as u64;
    DB_METRICS.query_count.fetch_add(1, Ordering::Relaxed);
    DB_METRICS
        .query_total_micros
        .fetch_add(micros, Ordering::Relaxed);
    DB_METRICS
        .query_max_micros
        .fetch_max(micros, Ordering::Relaxed);

    let millis = duration.as_millis();
    if millis < 1 {
        DB_METRICS.query_lt_1ms.fetch_add(1, Ordering::Relaxed);
    } else if millis < 5 {
        DB_METRICS.query_1_to_5ms.fetch_add(1, Ordering::Relaxed);
    } else if millis < 10 {
        DB_METRICS.query_5_to_10ms.fetch_add(1, Ordering::Relaxed);
    } else if millis < 25 {
        DB_METRICS.query_10_to_25ms.fetch_add(1, Ordering::Relaxed);
    } else {
        DB_METRICS.query_gte_25ms.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn reset_db_metrics() {
    DB_METRICS.acquire_count.store(0, Ordering::Relaxed);
    DB_METRICS.acquire_total_micros.store(0, Ordering::Relaxed);
    DB_METRICS.acquire_max_micros.store(0, Ordering::Relaxed);
    DB_METRICS.acquire_lt_1ms.store(0, Ordering::Relaxed);
    DB_METRICS.acquire_1_to_5ms.store(0, Ordering::Relaxed);
    DB_METRICS.acquire_5_to_10ms.store(0, Ordering::Relaxed);
    DB_METRICS.acquire_10_to_25ms.store(0, Ordering::Relaxed);
    DB_METRICS.acquire_25_to_50ms.store(0, Ordering::Relaxed);
    DB_METRICS.acquire_50_to_100ms.store(0, Ordering::Relaxed);
    DB_METRICS.acquire_gte_100ms.store(0, Ordering::Relaxed);

    DB_METRICS.query_count.store(0, Ordering::Relaxed);
    DB_METRICS.query_total_micros.store(0, Ordering::Relaxed);
    DB_METRICS.query_max_micros.store(0, Ordering::Relaxed);
    DB_METRICS.query_lt_1ms.store(0, Ordering::Relaxed);
    DB_METRICS.query_1_to_5ms.store(0, Ordering::Relaxed);
    DB_METRICS.query_5_to_10ms.store(0, Ordering::Relaxed);
    DB_METRICS.query_10_to_25ms.store(0, Ordering::Relaxed);
    DB_METRICS.query_gte_25ms.store(0, Ordering::Relaxed);
}

pub async fn acquire_conn(
    pool: &MySqlPool,
) -> Result<sqlx::pool::PoolConnection<sqlx::MySql>, sqlx::Error> {
    let start = std::time::Instant::now();
    let conn = pool.acquire().await?;
    record_pool_acquire(start.elapsed());
    Ok(conn)
}

#[derive(Serialize)]
pub struct PoolStats {
    pub configured_max: u32,
    pub size: u32,
    pub num_idle: usize,
    pub num_active: usize,
}

#[derive(Serialize)]
pub struct TimingSummary {
    pub count: u64,
    pub avg_ms: f64,
    pub max_ms: f64,
    pub distribution: std::collections::HashMap<String, u64>,
}

#[derive(Serialize)]
pub struct DbMetricsReport {
    pub pool: PoolStats,
    pub acquire: TimingSummary,
    pub query: TimingSummary,
}

pub fn get_db_metrics_report(pool: &MySqlPool) -> DbMetricsReport {
    let acq_count = DB_METRICS.acquire_count.load(Ordering::Relaxed);
    let acq_total_us = DB_METRICS.acquire_total_micros.load(Ordering::Relaxed);
    let acq_max_us = DB_METRICS.acquire_max_micros.load(Ordering::Relaxed);
    let acq_avg_ms = if acq_count > 0 {
        (acq_total_us as f64) / (acq_count as f64) / 1000.0
    } else {
        0.0
    };

    let mut acq_dist = std::collections::HashMap::new();
    acq_dist.insert(
        "<1ms".to_string(),
        DB_METRICS.acquire_lt_1ms.load(Ordering::Relaxed),
    );
    acq_dist.insert(
        "1-5ms".to_string(),
        DB_METRICS.acquire_1_to_5ms.load(Ordering::Relaxed),
    );
    acq_dist.insert(
        "5-10ms".to_string(),
        DB_METRICS.acquire_5_to_10ms.load(Ordering::Relaxed),
    );
    acq_dist.insert(
        "10-25ms".to_string(),
        DB_METRICS.acquire_10_to_25ms.load(Ordering::Relaxed),
    );
    acq_dist.insert(
        "25-50ms".to_string(),
        DB_METRICS.acquire_25_to_50ms.load(Ordering::Relaxed),
    );
    acq_dist.insert(
        "50-100ms".to_string(),
        DB_METRICS.acquire_50_to_100ms.load(Ordering::Relaxed),
    );
    acq_dist.insert(
        ">=100ms".to_string(),
        DB_METRICS.acquire_gte_100ms.load(Ordering::Relaxed),
    );

    let q_count = DB_METRICS.query_count.load(Ordering::Relaxed);
    let q_total_us = DB_METRICS.query_total_micros.load(Ordering::Relaxed);
    let q_max_us = DB_METRICS.query_max_micros.load(Ordering::Relaxed);
    let q_avg_ms = if q_count > 0 {
        (q_total_us as f64) / (q_count as f64) / 1000.0
    } else {
        0.0
    };

    let mut q_dist = std::collections::HashMap::new();
    q_dist.insert(
        "<1ms".to_string(),
        DB_METRICS.query_lt_1ms.load(Ordering::Relaxed),
    );
    q_dist.insert(
        "1-5ms".to_string(),
        DB_METRICS.query_1_to_5ms.load(Ordering::Relaxed),
    );
    q_dist.insert(
        "5-10ms".to_string(),
        DB_METRICS.query_5_to_10ms.load(Ordering::Relaxed),
    );
    q_dist.insert(
        "10-25ms".to_string(),
        DB_METRICS.query_10_to_25ms.load(Ordering::Relaxed),
    );
    q_dist.insert(
        ">=25ms".to_string(),
        DB_METRICS.query_gte_25ms.load(Ordering::Relaxed),
    );

    let pool_size = pool.size();
    let num_idle = pool.num_idle();
    let num_active = (pool_size as usize).saturating_sub(num_idle);

    DbMetricsReport {
        pool: PoolStats {
            configured_max: get_configured_pool_max(),
            size: pool_size,
            num_idle,
            num_active,
        },
        acquire: TimingSummary {
            count: acq_count,
            avg_ms: (acq_avg_ms * 100.0).round() / 100.0,
            max_ms: (acq_max_us as f64) / 1000.0,
            distribution: acq_dist,
        },
        query: TimingSummary {
            count: q_count,
            avg_ms: (q_avg_ms * 100.0).round() / 100.0,
            max_ms: (q_max_us as f64) / 1000.0,
            distribution: q_dist,
        },
    }
}
