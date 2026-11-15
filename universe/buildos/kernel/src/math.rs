// ===============================================================================
// BUILDOS KERNEL - MATH FUNCTIONS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Math functions for no_std environment
//!
//! Provides extension traits for f32 and f64 that wrap libm functions.

/// Extension trait for f32 math functions
pub trait F32Ext {
    fn sqrt(self) -> f32;
    fn sin(self) -> f32;
    fn cos(self) -> f32;
    fn tan(self) -> f32;
    fn round(self) -> f32;
    fn floor(self) -> f32;
    fn ceil(self) -> f32;
    fn abs(self) -> f32;
    fn powf(self, n: f32) -> f32;
    fn powi(self, n: i32) -> f32;
    fn log2(self) -> f32;
    fn log10(self) -> f32;
    fn ln(self) -> f32;
    fn exp(self) -> f32;
}

impl F32Ext for f32 {
    #[inline]
    fn sqrt(self) -> f32 {
        libm::sqrtf(self)
    }

    #[inline]
    fn sin(self) -> f32 {
        libm::sinf(self)
    }

    #[inline]
    fn cos(self) -> f32 {
        libm::cosf(self)
    }

    #[inline]
    fn tan(self) -> f32 {
        libm::tanf(self)
    }

    #[inline]
    fn round(self) -> f32 {
        libm::roundf(self)
    }

    #[inline]
    fn floor(self) -> f32 {
        libm::floorf(self)
    }

    #[inline]
    fn ceil(self) -> f32 {
        libm::ceilf(self)
    }

    #[inline]
    fn abs(self) -> f32 {
        libm::fabsf(self)
    }

    #[inline]
    fn powf(self, n: f32) -> f32 {
        libm::powf(self, n)
    }

    #[inline]
    fn powi(self, n: i32) -> f32 {
        libm::powf(self, n as f32)
    }

    #[inline]
    fn log2(self) -> f32 {
        libm::log2f(self)
    }

    #[inline]
    fn log10(self) -> f32 {
        libm::log10f(self)
    }

    #[inline]
    fn ln(self) -> f32 {
        libm::logf(self)
    }

    #[inline]
    fn exp(self) -> f32 {
        libm::expf(self)
    }
}

/// Extension trait for f64 math functions
pub trait F64Ext {
    fn sqrt(self) -> f64;
    fn sin(self) -> f64;
    fn cos(self) -> f64;
    fn tan(self) -> f64;
    fn round(self) -> f64;
    fn floor(self) -> f64;
    fn ceil(self) -> f64;
    fn abs(self) -> f64;
    fn powf(self, n: f64) -> f64;
    fn powi(self, n: i32) -> f64;
    fn log2(self) -> f64;
    fn log10(self) -> f64;
    fn ln(self) -> f64;
    fn exp(self) -> f64;
}

impl F64Ext for f64 {
    #[inline]
    fn sqrt(self) -> f64 {
        libm::sqrt(self)
    }

    #[inline]
    fn sin(self) -> f64 {
        libm::sin(self)
    }

    #[inline]
    fn cos(self) -> f64 {
        libm::cos(self)
    }

    #[inline]
    fn tan(self) -> f64 {
        libm::tan(self)
    }

    #[inline]
    fn round(self) -> f64 {
        libm::round(self)
    }

    #[inline]
    fn floor(self) -> f64 {
        libm::floor(self)
    }

    #[inline]
    fn ceil(self) -> f64 {
        libm::ceil(self)
    }

    #[inline]
    fn abs(self) -> f64 {
        libm::fabs(self)
    }

    #[inline]
    fn powf(self, n: f64) -> f64 {
        libm::pow(self, n)
    }

    #[inline]
    fn powi(self, n: i32) -> f64 {
        libm::pow(self, n as f64)
    }

    #[inline]
    fn log2(self) -> f64 {
        libm::log2(self)
    }

    #[inline]
    fn log10(self) -> f64 {
        libm::log10(self)
    }

    #[inline]
    fn ln(self) -> f64 {
        libm::log(self)
    }

    #[inline]
    fn exp(self) -> f64 {
        libm::exp(self)
    }
}
