# JVM Exporter

This project is a JVM exporter that utilizes `jps` and `jstat` to monitor the JVM metrics of running Java applications. It can be directly executed on the server to track key JVM-related metrics.

## How to Use

1. Run the exporter on your server.
2. Access the metrics endpoint at [http://127.0.0.1:9090/metrics](http://127.0.0.1:9090/metrics).

## Example Output

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
Each metric provides detailed insights into the JVM's garbage collection and memory usage, helping you monitor and optimize your Java applications.