# JVM Exporter

JVM Exporter, as a Prometheus exporter, is a robust monitoring tool designed to operate as an independent process,
providing detailed insights into Java Virtual Machine (JVM) metrics. Unlike tools such as jmx-exporter, which require
integration into Java applications to function, JVM Exporter runs separately. This design allows it to monitor all Java
processes on a server without modifying their execution environment or embedding code. Additionally, JVM Exporter can
monitor non-Java processes' CPU and memory usage, as well as server-level metrics such as CPU, disk, and network speed.
It also supports configuration centers, making it suitable for deployment across multiple server instances. Importantly,
JVM Exporter is specifically designed as an exporter for Prometheus, enabling seamless integration with Prometheus. This
integration offers a powerful, unified monitoring and alerting solution, making it possible to effectively track and
analyze JVM performance metrics within the Prometheus ecosystem.

## JVM-Exporter vs. JMX-Exporter

| Feature                         | JVM-Exporter                            | JMX-Exporter                                  |
|---------------------------------|-----------------------------------------|-----------------------------------------------|
| **Integration**                 | Runs as an independent process          | Requires integration into Java applications   |
| **Java Process Monitoring**     | Monitors all Java processes on a server | Monitors only the integrated Java application |
| **Non-Java Process Monitoring** | Yes (CPU, memory, upTime)               | No                                            |
| **Server-Level Metrics**        | Yes (CPU, disk, network speed, upTime)  | No                                            |
| **Configuration Centers**       | Supported                               | Not supported                                 |
| **Deployment**                  | Suitable for multiple server instances  | Limited to individual Java applications       |
| **Prometheus Integration**      | Seamless                                | Seamless                                      |

## Compile & Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/tf1997/jvm-exporter.git
   ```
2. Build the project (ensure you have Rust installed):
    ```bash
    cd jvm-exporter
    cargo build --release
    ```

## Using Precompiled Binaries

For convenience, precompiled binaries for Linux and Windows are provided.

### Linux

1. Download the binary:
   ```bash
   wget https://github.com/tf1997/jvm-exporter/releases/download/0.1/jvm-exporter
   ```
2. Make it executable:
   ```bash
   chmod +x jvm-exporter
   ```
3. Move it to an appropriate location:
   ```bash
   sudo mv jvm-exporter /usr/local/bin/jvm-exporter
   ```

## Usage

### command-line arguments

Start the JVM Exporter with configurable command-line arguments:

- `--java-home`: Set a custom JAVA_HOME.
- `--full-path`: By default, the full package path is displayed; this argument makes it display only the class name.
- `--auto-start`: Configure the program to auto-start with the system.

### configurable yaml file

The Configurable yaml file need to be placed in `/usr/local/jvm-exporter/config.yaml`

```yaml
configuration_service_url: http://127.0.0.1:29090/config
log_level: ERROR
system_processes:
  - Notability
  - WindowServer
```

note:

- `configuration_service_url` is come from the master jvm-exporter, non-master all can use its config. And, your local
  `system_processes` config will not be overwritten
- `system_processes` is system processes you want to monitoring

### Start the Service

   ```bash
   ./jvm-exporter
   ```

## View Metrics

Open your browser and visit http://localhost:29090/metrics to view the metrics.

### Grafana dashboard

The grafana dashboard is coming soon.

### Example Output

```plaintext
# HELP jstat_class_metrics Metrics from jstat -class
# TYPE jstat_class_metrics gauge
jstat_class_metrics{container="host",metric_name="Bytes",pid="31755",process_name="Main"} 5599
jstat_class_metrics{container="host",metric_name="Loaded",pid="31755",process_name="Main"} 80606
jstat_class_metrics{container="host",metric_name="Time",pid="31755",process_name="Main"} 22.76
jstat_class_metrics{container="host",metric_name="Unloaded",pid="31755",process_name="Main"} 5113
# HELP jstat_gc_metrics Metrics from jstat -gc
# TYPE jstat_gc_metrics gauge
jstat_gc_metrics{container="host",metric_name="CCSC",pid="31755",process_name="Main"} 57664
jstat_gc_metrics{container="host",metric_name="CCSU",pid="31755",process_name="Main"} 55127.6
jstat_gc_metrics{container="host",metric_name="CGC",pid="31755",process_name="Main"} 76
jstat_gc_metrics{container="host",metric_name="CGCT",pid="31755",process_name="Main"} 3.384
jstat_gc_metrics{container="host",metric_name="EC",pid="31755",process_name="Main"} 716800
jstat_gc_metrics{container="host",metric_name="EU",pid="31755",process_name="Main"} 663552
jstat_gc_metrics{container="host",metric_name="FGC",pid="31755",process_name="Main"} 0
jstat_gc_metrics{container="host",metric_name="FGCT",pid="31755",process_name="Main"} 0
jstat_gc_metrics{container="host",metric_name="GCT",pid="31755",process_name="Main"} 10.158
jstat_gc_metrics{container="host",metric_name="MC",pid="31755",process_name="Main"} 454336
jstat_gc_metrics{container="host",metric_name="MU",pid="31755",process_name="Main"} 448505.1
jstat_gc_metrics{container="host",metric_name="OC",pid="31755",process_name="Main"} 759808
jstat_gc_metrics{container="host",metric_name="OU",pid="31755",process_name="Main"} 678940.4
jstat_gc_metrics{container="host",metric_name="S0C",pid="31755",process_name="Main"} 0
jstat_gc_metrics{container="host",metric_name="S0U",pid="31755",process_name="Main"} 0
jstat_gc_metrics{container="host",metric_name="S1C",pid="31755",process_name="Main"} 30720
jstat_gc_metrics{container="host",metric_name="S1U",pid="31755",process_name="Main"} 30720
jstat_gc_metrics{container="host",metric_name="YGC",pid="31755",process_name="Main"} 134
jstat_gc_metrics{container="host",metric_name="YGCT",pid="31755",process_name="Main"} 6.774
# HELP jstat_gcutil_metrics Metrics from jstat -gcutil
# TYPE jstat_gcutil_metrics gauge
jstat_gcutil_metrics{container="host",metric_name="CCS",pid="31755",process_name="Main"} 95.6
jstat_gcutil_metrics{container="host",metric_name="CGC",pid="31755",process_name="Main"} 76
jstat_gcutil_metrics{container="host",metric_name="CGCT",pid="31755",process_name="Main"} 3.384
jstat_gcutil_metrics{container="host",metric_name="E",pid="31755",process_name="Main"} 92.57
jstat_gcutil_metrics{container="host",metric_name="FGC",pid="31755",process_name="Main"} 0
jstat_gcutil_metrics{container="host",metric_name="FGCT",pid="31755",process_name="Main"} 0
jstat_gcutil_metrics{container="host",metric_name="GCT",pid="31755",process_name="Main"} 10.158
jstat_gcutil_metrics{container="host",metric_name="M",pid="31755",process_name="Main"} 98.72
jstat_gcutil_metrics{container="host",metric_name="O",pid="31755",process_name="Main"} 89.36
jstat_gcutil_metrics{container="host",metric_name="S0",pid="31755",process_name="Main"} 0
jstat_gcutil_metrics{container="host",metric_name="S1",pid="31755",process_name="Main"} 100
jstat_gcutil_metrics{container="host",metric_name="YGC",pid="31755",process_name="Main"} 134
jstat_gcutil_metrics{container="host",metric_name="YGCT",pid="31755",process_name="Main"} 6.774
# HELP process_cpu_usage CPU usage percentage of the process
# TYPE process_cpu_usage gauge
process_cpu_usage{container="host",pid="31755",process_name="Main"} 0
process_cpu_usage{container="system",pid="377",process_name="WindowServer"} 0
# HELP process_memory_usage_bytes Memory usage in bytes of the process
# TYPE process_memory_usage_bytes gauge
process_memory_usage_bytes{container="host",pid="31755",process_name="Main"} 636764160
process_memory_usage_bytes{container="system",pid="377",process_name="WindowServer"} 0
# HELP process_memory_usage_percentage Memory usage percentage of the process
# TYPE process_memory_usage_percentage gauge
process_memory_usage_percentage{container="host",pid="31755",process_name="Main"} 2.4709701538085938
process_memory_usage_percentage{container="system",pid="377",process_name="WindowServer"} 0
# HELP process_start_time_seconds Start time of the process in seconds since the epoch
# TYPE process_start_time_seconds gauge
process_start_time_seconds{container="host",pid="31755",process_name="Main"} 1741415180
process_start_time_seconds{container="system",pid="377",process_name="WindowServer"} 0
# HELP process_up_time_seconds Up time of the process in seconds
# TYPE process_up_time_seconds gauge
process_up_time_seconds{container="host",pid="31755",process_name="Main"} 84531
process_up_time_seconds{container="system",pid="377",process_name="WindowServer"} 1741499711
# HELP system_cpu_usage_percentage Total system CPU usage percentage
# TYPE system_cpu_usage_percentage gauge
system_cpu_usage_percentage{cpu="cpu_0"} 34.87955856323242
system_cpu_usage_percentage{cpu="cpu_1"} 31.203964233398438
system_cpu_usage_percentage{cpu="cpu_2"} 24.033348083496094
system_cpu_usage_percentage{cpu="cpu_3"} 19.487337112426758
system_cpu_usage_percentage{cpu="cpu_4"} 6.520895004272461
system_cpu_usage_percentage{cpu="cpu_5"} 5.245965480804443
system_cpu_usage_percentage{cpu="cpu_6"} 3.537290573120117
system_cpu_usage_percentage{cpu="cpu_7"} 2.674703359603882
# HELP system_disk_usage_bytes Disk usage in bytes
# TYPE system_disk_usage_bytes gauge
system_disk_usage_bytes{disk="LM Studio 0.3.12-arm64",mount_point="/Volumes/LM Studio 0.3.12-arm64"} 1669468160
system_disk_usage_bytes{disk="Macintosh HD",mount_point="/"} 90802696473
system_disk_usage_bytes{disk="Macintosh HD",mount_point="/System/Volumes/Data"} 90802696473
# HELP system_memory_usage_bytes Total system memory usage in bytes
# TYPE system_memory_usage_bytes gauge
system_memory_usage_bytes{memory_type="used"} 21539209216
# HELP system_network_receive_bytes_per_sec Network receive rate in bytes per second
# TYPE system_network_receive_bytes_per_sec gauge
system_network_receive_bytes_per_sec{interface="anpi0"} 0
system_network_receive_bytes_per_sec{interface="anpi1"} 0
system_network_receive_bytes_per_sec{interface="ap1"} 0
system_network_receive_bytes_per_sec{interface="awdl0"} 0
system_network_receive_bytes_per_sec{interface="bridge0"} 0
system_network_receive_bytes_per_sec{interface="en0"} 24576
system_network_receive_bytes_per_sec{interface="en1"} 0
system_network_receive_bytes_per_sec{interface="en2"} 0
system_network_receive_bytes_per_sec{interface="en3"} 0
system_network_receive_bytes_per_sec{interface="en4"} 0
system_network_receive_bytes_per_sec{interface="gif0"} 0
system_network_receive_bytes_per_sec{interface="llw0"} 0
system_network_receive_bytes_per_sec{interface="lo0"} 225280
system_network_receive_bytes_per_sec{interface="stf0"} 0
system_network_receive_bytes_per_sec{interface="utun0"} 0
system_network_receive_bytes_per_sec{interface="utun1"} 0
system_network_receive_bytes_per_sec{interface="utun2"} 0
system_network_receive_bytes_per_sec{interface="utun3"} 0
system_network_receive_bytes_per_sec{interface="utun4"} 0
system_network_receive_bytes_per_sec{interface="utun5"} 0
# HELP system_network_transmit_bytes_per_sec Network transmit rate in bytes per second
# TYPE system_network_transmit_bytes_per_sec gauge
system_network_transmit_bytes_per_sec{interface="anpi0"} 0
system_network_transmit_bytes_per_sec{interface="anpi1"} 0
system_network_transmit_bytes_per_sec{interface="ap1"} 0
system_network_transmit_bytes_per_sec{interface="awdl0"} 0
system_network_transmit_bytes_per_sec{interface="bridge0"} 0
system_network_transmit_bytes_per_sec{interface="en0"} 10240
system_network_transmit_bytes_per_sec{interface="en1"} 0
system_network_transmit_bytes_per_sec{interface="en2"} 0
system_network_transmit_bytes_per_sec{interface="en3"} 0
system_network_transmit_bytes_per_sec{interface="en4"} 0
system_network_transmit_bytes_per_sec{interface="gif0"} 0
system_network_transmit_bytes_per_sec{interface="llw0"} 0
system_network_transmit_bytes_per_sec{interface="lo0"} 225280
system_network_transmit_bytes_per_sec{interface="stf0"} 0
system_network_transmit_bytes_per_sec{interface="utun0"} 0
system_network_transmit_bytes_per_sec{interface="utun1"} 0
system_network_transmit_bytes_per_sec{interface="utun2"} 0
system_network_transmit_bytes_per_sec{interface="utun3"} 0
system_network_transmit_bytes_per_sec{interface="utun4"} 0
system_network_transmit_bytes_per_sec{interface="utun5"} 0
# HELP system_swap_usage_bytes Used swap memory in bytes
# TYPE system_swap_usage_bytes gauge
system_swap_usage_bytes{swap_type="used"} 8780185600
# HELP system_total_disk_bytes Total disk space in bytes
# TYPE system_total_disk_bytes gauge
system_total_disk_bytes{disk="LM Studio 0.3.12-arm64",mount_point="/Volumes/LM Studio 0.3.12-arm64"} 2518507520
system_total_disk_bytes{disk="Macintosh HD",mount_point="/"} 494384795648
system_total_disk_bytes{disk="Macintosh HD",mount_point="/System/Volumes/Data"} 494384795648
# HELP system_total_memory_bytes Total system memory in bytes
# TYPE system_total_memory_bytes gauge
system_total_memory_bytes{memory_type="total"} 25769803776
# HELP system_total_swap_bytes Total swap memory in bytes
# TYPE system_total_swap_bytes gauge
system_total_swap_bytes{swap_type="total"} 9663676416
# HELP system_uptime_seconds Total system uptime in seconds
# TYPE system_uptime_seconds gauge
system_uptime_seconds{type="system"} 189745
```

## FAQ

**Q: How do I resolve a jps command failure?**

A: Ensure that the `JAVA_HOME` environment variable is correctly set and that jps is accessible in your PATH.

**Q: What if the metrics are not updating?**

A: Check that the JVM processes are running and that jvm-exporter has sufficient permissions to access the jstat
command.

## License

This project is licensed under the Apache License 2.0 - see the LICENSE file for details.