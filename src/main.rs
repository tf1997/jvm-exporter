mod monitor;
mod routes;
mod config;

mod deploy {
    pub mod deploy;
}
mod metrics {
    pub mod collect;
    pub mod metrics;
    pub mod timer;
}

fn main() {
    monitor::main()
}