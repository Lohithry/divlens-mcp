use sysinfo::System;

pub fn get_cpu_usage(sys: &System) -> f32 {
    sys.global_cpu_usage()
}

