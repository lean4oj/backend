define void @hash_add(
    ptr noundef nonnull align 8 captures(none) dereferenceable(32) %a,
    ptr noundef nonnull readonly align 8 captures(none) dereferenceable(32) %b
) inlinehint nounwind alwaysinline {
    %c = load i256, ptr %a, align 8
    %d = load i256, ptr %b, align 8
    %e = add i256 %c, %d
    store i256 %e, ptr %a, align 8
    ret void
}
