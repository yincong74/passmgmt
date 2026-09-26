//! pmgr-core: 密码管理器核心逻辑（数据模型、加密、密码生成），不依赖任何 GUI API。

pub const VERSION: &str = "0.1.0";

#[cfg(test)]
mod smoke_tests {
    #[test]
    fn version_is_set() {
        assert_eq!(super::VERSION, "0.1.0");
    }
}
