#[cfg(test)]
mod example_bench_test {
    use benchkit::run_all_benchmarks;
    use rand::Rng;

    #[test]
    fn test_example_benchmark() {
        let mut rng = rand::rng();
        let pr_number = rng.random_range(10000..100000);
        let run_id = rng.random_range(100000000..1000000000);

        let config_path = "example.benchmark.yml";
        let result = run_all_benchmarks(config_path, Some(pr_number), Some(run_id));
        result.expect("Benchmarks failed unexpectedly");
    }
}
