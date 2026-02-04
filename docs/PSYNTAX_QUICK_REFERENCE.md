# Quick Reference: psyntax.scm Migration Guide

## Overview

This document provides a comprehensive guide for understanding and potentially replacing Grift's syntax-case implementation with elements from psyntax.scm.

**Location**: `docs/PSYNTAX_MIGRATION_GUIDE.md`

## Key Findings

### Current State
- Grift has a **functional** procedural macro system with `syntax-case`
- Implementation: ~2000 lines of Rust in `expand.rs` and `forms.rs`
- Status: ✅ Complete for core features (Phases 1-9)

### psyntax.scm
- Reference implementation from Guile (3,196 lines of Scheme)
- Origin: Chez Scheme (Dybvig et al., 1992)
- License: GNU LGPL 3.0
- Full R6RS/R7RS macro system with module integration

### Critical Recommendation

**⚠️ DO NOT attempt full port of psyntax.scm to Grift**

Reasons:
- Memory model incompatible (heap vs. fixed arena)
- Requires module system (not applicable to Grift)
- Excessive complexity (3,196 lines vs. current ~2,000)
- Self-hosting requirement conflicts with Rust implementation

### Alternative: Selective Enhancement

Extract specific improvements:
1. ✅ Enhanced pattern matching (`$sc-dispatch` algorithm)
2. ✅ Anti-mark hygiene mechanism
3. ✅ Nested ellipsis support (fixes Phase 7 bug)
4. ✅ Better error messages

Timeline: 5-7 weeks
Risk: Medium (mitigated by testing)
Value: High (fixes bugs, enables advanced macros)

## Document Structure

1. **Executive Summary** - High-level overview
2. **Current State Analysis** - Grift's implementation details
3. **psyntax.scm Architecture** - Deep dive into reference implementation
4. **Compatibility Analysis** - What works, what doesn't
5. **Migration Strategy** - Phased selective enhancement approach
6. **Implementation Roadmap** - 7-week timeline
7. **Testing Strategy** - Validation and regression testing
8. **Risk Assessment** - Technical and project risks
9. **Appendices** - References, glossary, further reading

## Quick Stats

| Metric | Value |
|--------|-------|
| **Document size** | 1,066 lines, 36KB |
| **Sections** | 9 major, 81 headings |
| **Code examples** | 20+ |
| **Tables** | 15+ comparison matrices |
| **References** | Academic papers, implementation docs |

## Key Takeaways

### For Implementers
- Focus on **algorithm extraction**, not direct port
- Maintain **backward compatibility** (all tests must pass)
- Use **feature flags** for gradual rollout
- **Benchmark** performance at each phase

### For Users
- Current system is **production-ready**
- Enhancements will **fix bugs** and **enable advanced patterns**
- **No breaking changes** expected
- Timeline: Q2-Q3 2026 (if pursued)

### For Researchers
- Demonstrates **no_std adaptation** of classic algorithms
- Shows **arena allocation** trade-offs
- Documents **simplification strategies** for embedded Scheme

## Related Documentation

- `HYGIENIC_MACROS_IMPLEMENTATION.md` - Implementation log (Phases 1-9)
- `EXTENDING_SCHEME_MACROS.md` - Advanced patterns and syntax-case
- `SCHEME_R7RS_CONFORMANCE.md` - Standards compliance
- `ARENA_ARCHITECTURE.md` - Memory management
- `LISP_ARCHITECTURE.md` - Evaluator design

## How to Use This Guide

### If you want to understand psyntax.scm:
→ Read **Section 3: psyntax.scm Architecture**

### If you want to know compatibility:
→ Read **Section 4: Compatibility Analysis**

### If you want to implement improvements:
→ Read **Section 5: Migration Strategy** and **Section 6: Implementation Roadmap**

### If you want to see code examples:
→ Check **Appendix sections** and algorithm subsections

### If you're evaluating feasibility:
→ Read **Executive Summary** and **Section 8: Risk Assessment**

## Quick Decision Matrix

| Goal | Action | Document Section |
|------|--------|-----------------|
| Fix nested ellipsis bug | Implement Phase 1.3 | Migration Strategy |
| Add vector patterns | Implement Phase 1.1 | Pattern Matching |
| Improve error messages | Implement Phase 2 | Enhanced Error Reporting |
| Better hygiene | Implement Phase 1.2 | Anti-mark Mechanism |
| Understand psyntax | Read full guide | psyntax Architecture |
| Assess feasibility | Check matrices | Compatibility Analysis |

## Citation

If referencing this work:

```
Grift Development Team. (2026). psyntax.scm Migration Guide.
https://github.com/gold-silver-copper/grift/blob/main/docs/PSYNTAX_MIGRATION_GUIDE.md
```

Original psyntax implementation:
```
R. Kent Dybvig, Oscar Waddell, Bob Hieb, Carl Bruggeman. (1992-2024).
Portable Syntax Expander (psyntax). GNU Guile Project.
Based on: Dybvig, Hieb, Bruggeman. "Syntax Abstraction in Scheme."
Lisp and Symbolic Computation 5:4, 295-326, 1992.
```

---

**Last Updated**: 2026-02-04  
**Status**: Complete  
**Next Steps**: Review by maintainers, community feedback
