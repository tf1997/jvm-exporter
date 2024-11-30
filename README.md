# JVM Exporter

JVM Exporter, as a prometheus exporter, is a robust monitoring tool designed to operate as an independent process, providing detailed insights into
Java Virtual Machine (JVM) metrics. Unlike tools such as jmx-exporter, which require integration into Java applications
to function, JVM Exporter runs separately. This design allows it to monitor multiple Java applications without modifying
their execution environment or embedding code. Importantly, JVM Exporter is specifically designed as an exporter for
Prometheus, enabling seamless integration with Prometheus. This integration offers a powerful, unified monitoring and
alerting solution, making it possible to effectively track and analyze JVM performance metrics within the Prometheus
ecosystem.

## Key Features and Advantages

- **Standalone Operation**: Operates as a separate process, eliminating the need for embedding within or altering Java
  applications.
- **Customizable JAVA_HOME**: Allows users to specify the Java installation path, ensuring compatibility with different
  Java environments.
- **Flexible Display Options**: Users can choose to display either the full package path or just the class name of Java
  processes, catering to different levels of detail requirements.
- **Auto-Start Capability**: Can be configured to automatically start with the system using system services, providing
  continuous monitoring without manual intervention.

## Why Choose JVM Exporter?

JVM Exporter's unique standalone capability, combined with its flexibility and ease of use, makes it a superior choice
for monitoring JVM metrics. It is particularly beneficial in scenarios where direct interaction with the application is
not possible or desired. By providing detailed insights into JVM performance with minimal setup, JVM Exporter helps
maintain and improve application performance and stability.

## Installation

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

Start the JVM Exporter with configurable command-line arguments:

- `--java-home`: Set a custom JAVA_HOME.
- `--full-path`: By default, the full package path is displayed; this argument makes it display only the class name.
- `--auto-start`: Configure the program to auto-start with the system.

### Start the Service

   ```bash
   ./jvm-exporter --auto-start
   ```

## View Metrics

Open your browser and visit http://localhost:29090/metrics to view the metrics.

### Example Output

```plaintext
# HELP jstat_gc_metrics Metrics from jstat -gc
# TYPE jstat_gc_metrics gauge
jstat_gc_metrics{metric_name="CCSC",pid="32340",process_name="com.intellij.idea.Main"} 60096
jstat_gc_metrics{metric_name="CCSU",pid="32340",process_name="com.intellij.idea.Main"} 57417
jstat_gc_metrics{metric_name="EC",pid="32340",process_name="com.intellij.idea.Main"} 291840
jstat_gc_metrics{metric_name="EU",pid="32340",process_name="com.intellij.idea.Main"} 74752
jstat_gc_metrics{metric_name="FGC",pid="32340",process_name="com.intellij.idea.Main"} 2
jstat_gc_metrics{metric_name="FGCT",pid="32340",process_name="com.intellij.idea.Main"} 0
jstat_gc_metrics{metric_name="GCT",pid="32340",process_name="com.intellij.idea.Main"} 4
jstat_gc_metrics{metric_name="MC",pid="32340",process_name="com.intellij.idea.Main"} 480704
jstat_gc_metrics{metric_name="MU",pid="32340",process_name="com.intellij.idea.Main"} 474517
jstat_gc_metrics{metric_name="OC",pid="32340",process_name="com.intellij.idea.Main"} 738304
jstat_gc_metrics{metric_name="OU",pid="32340",process_name="com.intellij.idea.Main"} 679890
jstat_gc_metrics{metric_name="S0C",pid="32340",process_name="com.intellij.idea.Main"} 0
jstat_gc_metrics{metric_name="S0U",pid="32340",process_name="com.intellij.idea.Main"} 0
jstat_gc_metrics{metric_name="S1C",pid="32340",process_name="com.intellij.idea.Main"} 18432
jstat_gc_metrics{metric_name="S1U",pid="32340",process_name="com.intellij.idea.Main"} 18071
jstat_gc_metrics{metric_name="YGC",pid="32340",process_name="com.intellij.idea.Main"} 318
jstat_gc_metrics{metric_name="YGCT",pid="32340",process_name="com.intellij.idea.Main"} 4
```

Each metric provides detailed insights into the JVM's garbage collection and memory usage, helping you monitor and
optimize your Java applications.

## FAQ

**Q: How do I resolve a jps command failure?**

A: Ensure that the `JAVA_HOME` environment variable is correctly set and that jps is accessible in your PATH.

**Q: What if the metrics are not updating?**

A: Check that the JVM processes are running and that jvm-exporter has sufficient permissions to access the jstat
command.

## License

This project is licensed under the Apache License 2.0 - see the LICENSE file for details.