use super::*;

pub(super) fn parse_comment_mode(arguments: &Value) -> CommentMode {
    if arguments
        .get("include_comments")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return CommentMode::Leading;
    }

    CommentMode::from_option(arguments.get("comment_mode").and_then(Value::as_str))
}

pub(super) fn apply_comment_mode(
    shape: &mut EnhancedFileShape,
    source: &str,
    language: crate::parser::Language,
    comment_mode: CommentMode,
) {
    if comment_mode == CommentMode::None {
        return;
    }

    for func in &mut shape.functions {
        func.code = prepend_leading_comments_to_code(
            source,
            func.line,
            language,
            func.code.take(),
            comment_mode,
        );
    }

    for struct_info in &mut shape.structs {
        struct_info.code = prepend_leading_comments_to_code(
            source,
            struct_info.line,
            language,
            struct_info.code.take(),
            comment_mode,
        );
    }

    for class in &mut shape.classes {
        class.code = prepend_leading_comments_to_code(
            source,
            class.line,
            language,
            class.code.take(),
            comment_mode,
        );
        for method in &mut class.methods {
            method.code = prepend_leading_comments_to_code(
                source,
                method.line,
                language,
                method.code.take(),
                comment_mode,
            );
        }
    }

    for interface in &mut shape.interfaces {
        interface.code = prepend_leading_comments_to_code(
            source,
            interface.line,
            language,
            interface.code.take(),
            comment_mode,
        );
        for method in &mut interface.methods {
            method.code = prepend_leading_comments_to_code(
                source,
                method.line,
                language,
                method.code.take(),
                comment_mode,
            );
        }
    }

    for tr in &mut shape.traits {
        for method in &mut tr.methods {
            method.code = prepend_leading_comments_to_code(
                source,
                method.line,
                language,
                method.code.take(),
                comment_mode,
            );
        }
    }

    for block in &mut shape.impl_blocks {
        for method in &mut block.methods {
            method.code = prepend_leading_comments_to_code(
                source,
                method.line,
                language,
                method.code.take(),
                comment_mode,
            );
        }
    }
}
