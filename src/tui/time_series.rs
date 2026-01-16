//! Time series storage for historical data points

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

/// A single data point in the time series
#[derive(Debug, Clone, Copy)]
pub struct DataPoint {
    pub timestamp: i64,
    pub value: i64,
}

/// Storage for time series data organized by job_id and operation
#[derive(Debug, Clone)]
pub struct TimeSeriesStore {
    /// Map: job_id -> operation -> Vec<DataPoint>
    data: HashMap<String, HashMap<String, VecDeque<DataPoint>>>,
    /// Maximum age of data points to keep
    #[allow(dead_code)]
    max_age: Duration,
    /// Maximum number of points per series
    max_points: usize,
}

impl TimeSeriesStore {
    pub fn new(max_age: Duration) -> Self {
        Self {
            data: HashMap::new(),
            max_age,
            max_points: 1000, // Limit memory usage
        }
    }

    /// Insert a new data point
    pub fn insert(&mut self, job_id: &str, operation: &str, timestamp: i64, value: i64) {
        let job_data = self.data.entry(job_id.to_string()).or_default();
        let op_data = job_data.entry(operation.to_string()).or_default();

        // Add new point
        op_data.push_back(DataPoint { timestamp, value });

        // Limit size
        while op_data.len() > self.max_points {
            op_data.pop_front();
        }
    }

    /// Get time series data for a specific job and operation
    #[allow(dead_code)]
    pub fn get_series(&self, job_id: &str, operation: &str, since: i64) -> Vec<DataPoint> {
        self.data
            .get(job_id)
            .and_then(|job_data| job_data.get(operation))
            .map(|points| {
                points
                    .iter()
                    .filter(|p| p.timestamp >= since)
                    .copied()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get aggregated time series for a job (sum of selected operations)
    #[allow(dead_code)]
    pub fn get_aggregated_series(
        &self,
        job_id: &str,
        operations: &[&str],
        since: i64,
    ) -> Vec<DataPoint> {
        let Some(job_data) = self.data.get(job_id) else {
            return vec![];
        };

        // Collect all timestamps
        let mut all_timestamps: Vec<i64> = job_data
            .iter()
            .filter(|(op, _)| operations.contains(&op.as_str()))
            .flat_map(|(_, points)| points.iter().map(|p| p.timestamp))
            .filter(|&ts| ts >= since)
            .collect();

        all_timestamps.sort_unstable();
        all_timestamps.dedup();

        // Build aggregated series
        all_timestamps
            .into_iter()
            .map(|ts| {
                let value: i64 = operations
                    .iter()
                    .filter_map(|&op| job_data.get(op))
                    .filter_map(|points| points.iter().find(|p| p.timestamp == ts))
                    .map(|p| p.value)
                    .sum();
                DataPoint {
                    timestamp: ts,
                    value,
                }
            })
            .collect()
    }

    /// Get time series for a specific operation across all jobs (for aggregated mode)
    /// Returns a single series with values summed across all jobs for each timestamp
    pub fn get_aggregated_operation_series(&self, operation: &str, since: i64) -> Vec<DataPoint> {
        // Collect all data points for this operation across all jobs
        let mut by_timestamp: HashMap<i64, i64> = HashMap::new();

        for job_data in self.data.values() {
            if let Some(points) = job_data.get(operation) {
                for point in points.iter() {
                    if point.timestamp >= since {
                        *by_timestamp.entry(point.timestamp).or_default() += point.value;
                    }
                }
            }
        }

        // Convert to sorted vector
        let mut result: Vec<DataPoint> = by_timestamp
            .into_iter()
            .map(|(timestamp, value)| DataPoint { timestamp, value })
            .collect();
        result.sort_by_key(|p| p.timestamp);
        result
    }

    /// Get time series for a specific job and operation (for per-operation mode)
    pub fn get_job_operation_series(
        &self,
        job_id: &str,
        operation: &str,
        since: i64,
    ) -> Vec<DataPoint> {
        self.data
            .get(job_id)
            .and_then(|job_data| job_data.get(operation))
            .map(|points| {
                points
                    .iter()
                    .filter(|p| p.timestamp >= since)
                    .copied()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all job IDs that have data for any of the specified operations
    #[allow(dead_code)]
    pub fn jobs_with_operations(&self, operations: &[&str], since: i64) -> Vec<String> {
        self.data
            .iter()
            .filter(|(_, job_data)| {
                operations.iter().any(|&op| {
                    job_data
                        .get(op)
                        .map(|points| points.iter().any(|p| p.timestamp >= since))
                        .unwrap_or(false)
                })
            })
            .map(|(job_id, _)| job_id.clone())
            .collect()
    }

    /// Calculate rate (ops/sec) from a series of data points
    /// Returns a new series where each point's value is the rate between consecutive points
    pub fn calculate_rate(series: &[DataPoint]) -> Vec<DataPoint> {
        if series.len() < 2 {
            return vec![];
        }

        series
            .windows(2)
            .filter_map(|window| {
                let prev = &window[0];
                let curr = &window[1];
                let time_delta = curr.timestamp - prev.timestamp;

                if time_delta > 0 {
                    // Calculate rate: (current - previous) / time_delta
                    // Handle counter wrap-around or resets by using max(0, diff)
                    let value_diff = (curr.value - prev.value).max(0);
                    let rate = value_diff / time_delta;
                    Some(DataPoint {
                        timestamp: curr.timestamp,
                        value: rate,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Prune data points older than the cutoff timestamp
    pub fn prune_before(&mut self, cutoff: i64) {
        for job_data in self.data.values_mut() {
            for points in job_data.values_mut() {
                while let Some(front) = points.front() {
                    if front.timestamp < cutoff {
                        points.pop_front();
                    } else {
                        break;
                    }
                }
            }
        }

        // Remove empty entries
        self.data.retain(|_, job_data| {
            job_data.retain(|_, points| !points.is_empty());
            !job_data.is_empty()
        });
    }

    /// Get all job IDs in the store
    #[allow(dead_code)]
    pub fn job_ids(&self) -> Vec<&str> {
        self.data.keys().map(|s| s.as_str()).collect()
    }

    /// Check if we have data for a job
    #[allow(dead_code)]
    pub fn has_job(&self, job_id: &str) -> bool {
        self.data.contains_key(job_id)
    }

    /// Get the time range of data in the store
    #[allow(dead_code)]
    pub fn time_range(&self) -> Option<(i64, i64)> {
        let mut min_ts = i64::MAX;
        let mut max_ts = i64::MIN;

        for job_data in self.data.values() {
            for points in job_data.values() {
                if let Some(first) = points.front() {
                    min_ts = min_ts.min(first.timestamp);
                }
                if let Some(last) = points.back() {
                    max_ts = max_ts.max(last.timestamp);
                }
            }
        }

        if min_ts <= max_ts {
            Some((min_ts, max_ts))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_rate_empty() {
        let series: Vec<DataPoint> = vec![];
        let result = TimeSeriesStore::calculate_rate(&series);
        assert!(result.is_empty());
    }

    #[test]
    fn test_calculate_rate_single_point() {
        let series = vec![DataPoint {
            timestamp: 100,
            value: 1000,
        }];
        let result = TimeSeriesStore::calculate_rate(&series);
        assert!(result.is_empty());
    }

    #[test]
    fn test_calculate_rate_two_points() {
        let series = vec![
            DataPoint {
                timestamp: 100,
                value: 1000,
            },
            DataPoint {
                timestamp: 105,
                value: 1500,
            },
        ];
        let result = TimeSeriesStore::calculate_rate(&series);
        assert_eq!(result.len(), 1);
        // Rate = (1500 - 1000) / (105 - 100) = 500 / 5 = 100
        assert_eq!(result[0].value, 100);
        assert_eq!(result[0].timestamp, 105);
    }

    #[test]
    fn test_calculate_rate_multiple_points() {
        let series = vec![
            DataPoint {
                timestamp: 100,
                value: 1000,
            },
            DataPoint {
                timestamp: 110,
                value: 2000,
            },
            DataPoint {
                timestamp: 120,
                value: 2500,
            },
        ];
        let result = TimeSeriesStore::calculate_rate(&series);
        assert_eq!(result.len(), 2);
        // First rate = (2000 - 1000) / 10 = 100
        assert_eq!(result[0].value, 100);
        // Second rate = (2500 - 2000) / 10 = 50
        assert_eq!(result[1].value, 50);
    }

    #[test]
    fn test_calculate_rate_counter_reset() {
        // Counter reset (current < previous) should result in 0 rate
        let series = vec![
            DataPoint {
                timestamp: 100,
                value: 1000,
            },
            DataPoint {
                timestamp: 110,
                value: 500, // Counter reset
            },
        ];
        let result = TimeSeriesStore::calculate_rate(&series);
        assert_eq!(result.len(), 1);
        // saturating_sub handles this: 500 - 1000 = 0
        assert_eq!(result[0].value, 0);
    }
}
