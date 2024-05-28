use biome_analyze::{context::RuleContext, declare_rule, Ast, Rule, RuleDiagnostic, RuleSource};
use biome_console::markup;
use biome_deserialize_macros::Deserializable;
use biome_js_syntax::{
    AnyJsxAttribute, AnyJsxAttributeName, AnyJsxAttributeValue, AnyJsxTag, JsxAttribute,
    JsxAttributeList, JsxName, JsxReferenceIdentifier, JsxText,
};
use biome_rowan::AstNode;
use serde::{Deserialize, Serialize};

declare_rule! {
    /// Enforce that a label element or component has a text label and an associated input.
    ///
    /// An "input" is considered one of the following elements: `input`, `meter`, `output`, `progress`, `select` or `textarea`
    ///
    /// There are two supported ways to associate a label with an input:
    /// - Wrapping an input in a label element.
    /// - Adding a `for` attribute (or `htmlFor` in React) to a label and assigning it a DOM ID string associated with an input on the page.
    ///
    ///
    /// This rule checks that any `label` element (or an indicated custom component that will output a `label` element) meets one of these conditions:
    /// - Wraps an `input` element (or an indicated custom component that will output an `input` element)
    /// - Has a `for` or `htmlFor` attribute and that the `label` element/component has accessible text content.
    ///
    /// ## Examples
    ///
    /// ### Invalid
    ///
    /// ```jsx,expect_diagnostic
    /// <label for="js_id" />;
    /// ```
    ///
    /// ```jsx,expect_diagnostic
    /// <label for="js_id"><input /></label>;
    /// ```
    ///
    /// ```jsx,expect_diagnostic
    /// <label htmlFor="js_id" />;
    /// ```
    ///
    /// ```jsx,expect_diagnostic
    /// <label htmlFor="js_id"><input /></label>;
    /// ```
    ///
    /// ```jsx,expect_diagnostic
    /// <label>A label</label>;
    /// ```
    ///
    /// ```jsx,expect_diagnostic
    /// <div><label /><input /></div>;
    /// ```
    ///
    /// ### Valid
    ///
    /// ```jsx
    /// <label for="js_id" aria-label="A label" />;
    /// <label for="js_id" aria-labelledby="A label" />;
    /// <label htmlFor="js_id" aria-label="A label" />;
    /// <label htmlFor="js_id" aria-labelledby="A label" />;
    /// <label>A label<input /></label>;
    /// <label>A label<textarea /></label>;
    /// <label><img alt="A label" /><input /></label>;
    /// ```
    ///
    /// ## Options
    ///
    /// The rule supports the following options:
    /// - `inputComponents` - An array of component names that should be considered the same as an `input` element.
    /// - `labelAttributes` - An array of attributes that should be treated as the `label` accessible text content.
    /// - `labelComponents` - An array of component names that should be considered the same as a `label` element.
    ///
    /// ```json
    /// {
    ///     "//": "...",
    ///     "options": {
    ///         "inputComponents": ["CustomInput"],
    ///         "labelAttributes": ["label"],
    ///         "labelComponents": ["CustomLabel"]
    ///     }
    /// }
    /// ```
    ///
    pub NoLabelWithoutControl {
        version: "next",
        name: "noLabelWithoutControl",
        language: "jsx",
        sources: &[RuleSource::EslintJsxA11y("label-has-associated-control")],
        recommended: false,
    }
}

#[derive(Clone, Debug, Default, Deserialize, Deserializable, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoLabelWithoutControlOptions {
    /// Array of component names that should be considered the same as an `input` element.
    pub input_components: Vec<String>,
    /// Array of attributes that should be treated as the `label` accessible text content.
    pub label_attributes: Vec<String>,
    /// Array of component names that should be considered the same as a `label` element.
    pub label_components: Vec<String>,
}

pub struct NoLabelWithoutControlState {
    pub has_text_content: bool,
    pub has_control_association: bool,
}

impl Rule for NoLabelWithoutControl {
    type Query = Ast<AnyJsxTag>;
    type State = NoLabelWithoutControlState;
    type Signals = Option<Self::State>;
    type Options = NoLabelWithoutControlOptions;

    fn run(ctx: &RuleContext<Self>) -> Self::Signals {
        let node = ctx.query();
        let options = ctx.options();
        let label_attributes = get_option(&options.label_attributes, &["aria-label", "alt"]);
        let label_components = get_option(&options.label_components, &["label"]);
        let input_components = get_option(
            &options.input_components,
            &["input", "meter", "output", "progress", "select", "textarea"],
        );
        let element_name = get_element_name(node)?;
        let is_allowed_element = label_components.contains(&element_name);

        if !is_allowed_element {
            return None;
        }

        let has_text_content = has_accessible_label(node, &label_attributes);
        let has_control_association =
            has_for_attribute(node)? || has_nested_control(node, &input_components);

        if has_text_content && has_control_association {
            return None;
        }

        Some(NoLabelWithoutControlState {
            has_text_content,
            has_control_association,
        })
    }

    fn diagnostic(ctx: &RuleContext<Self>, state: &Self::State) -> Option<RuleDiagnostic> {
        let node = ctx.query();
        let mut diagnostic = RuleDiagnostic::new(
            rule_category!(),
            node.range(),
            markup! {
                "A form label must be associated with an input."
            },
        );

        if !state.has_text_content {
            diagnostic = diagnostic.note(
                markup! { "Consider adding an accessible text content to the label element." },
            );
        }

        if !state.has_control_association {
            diagnostic = diagnostic.note(
                markup! { "Consider adding a `for` or `htmlFor` attribute to the label element or moving the input element to inside the label element." },
            );
        }

        Some(diagnostic)
    }
}

/// Returns an array option merged with the default value
fn get_option(value: &[String], default_value: &[&str]) -> Vec<String> {
    value
        .iter()
        .cloned()
        .chain(default_value.iter().map(|value| (*value).to_string()))
        .collect()
}

/// Returns the `JsxAttributeList` of the passed `AnyJsxTag`
fn get_element_attributes(jsx_tag: &AnyJsxTag) -> Option<JsxAttributeList> {
    match jsx_tag {
        AnyJsxTag::JsxElement(element) => Some(element.opening_element().ok()?.attributes()),
        AnyJsxTag::JsxSelfClosingElement(element) => Some(element.attributes()),
        _ => None,
    }
}

/// Returns the element name of a `AnyJsxTag`
fn get_element_name(jsx_tag: &AnyJsxTag) -> Option<String> {
    match jsx_tag {
        AnyJsxTag::JsxElement(element) => Some(element.opening_element().ok()?.name().ok()?.text()),
        AnyJsxTag::JsxSelfClosingElement(element) => Some(element.name().ok()?.text()),
        _ => None,
    }
}

/// Returns whether the passed `AnyJsxTag` have a `for` or `htmlFor` attribute
fn has_for_attribute(jsx_tag: &AnyJsxTag) -> Option<bool> {
    let for_attributes = &["for", "htmlFor"];
    let attributes = get_element_attributes(jsx_tag)?;

    Some(attributes.into_iter().any(|attribute| {
        match attribute {
            AnyJsxAttribute::JsxAttribute(jsx_attribute) => jsx_attribute
                .name()
                .is_ok_and(|jsx_name| for_attributes.contains(&jsx_name.text().as_str())),
            _ => false,
        }
    }))
}

/// Returns whether the passed `AnyJsxTag` have a child that is considered an input component
/// according to the passed `input_components` parameter
fn has_nested_control(jsx_tag: &AnyJsxTag, input_components: &[String]) -> bool {
    jsx_tag.syntax().descendants().any(|descendant| {
        match (
            JsxName::cast(descendant.clone()),
            JsxReferenceIdentifier::cast(descendant.clone()),
        ) {
            (Some(jsx_name), _) => input_components.contains(&jsx_name.text()),
            (_, Some(jsx_reference_identifier)) => {
                input_components.contains(&jsx_reference_identifier.text())
            }
            _ => false,
        }
    })
}

/// Returns whether the passed `AnyJsxTag` meets one of the following conditions:
/// - Has a label attribute that corresponds to the `label_attributes` parameter
/// - Has an `aria-labelledby` attribute
/// - Has a child `JsxText` node
fn has_accessible_label(jsx_tag: &AnyJsxTag, label_attributes: &[String]) -> bool {
    let jsx_attributes = jsx_tag
        .syntax()
        .descendants()
        .filter_map(JsxAttribute::cast)
        .collect::<Vec<JsxAttribute>>();

    let has_accessible_attribute = jsx_attributes.iter().any(|jsx_attribute| {
        match (
            jsx_attribute.name().ok(),
            jsx_attribute
                .initializer()
                .and_then(|initializer| initializer.value().ok()),
        ) {
            (Some(AnyJsxAttributeName::JsxName(jsx_name)), Some(jsx_attribute_value)) => {
                let has_label_attribute = label_attributes.contains(&jsx_name.text());
                let is_aria_labelledby_attribute = jsx_name.text() == "aria-labelledby";
                let has_value = has_jsx_attribute_value(jsx_attribute_value);

                has_value && (is_aria_labelledby_attribute || has_label_attribute)
            }
            _ => false,
        }
    });

    let has_text = jsx_tag
        .syntax()
        .descendants()
        .any(|descendant| JsxText::cast(descendant).is_some());

    has_accessible_attribute || has_text
}

/// Returns whether the passed `jsx_attribute_value` has a valid value inside it
fn has_jsx_attribute_value(jsx_attribute_value: AnyJsxAttributeValue) -> bool {
    match jsx_attribute_value {
        AnyJsxAttributeValue::JsxString(jsx_string) => jsx_string
            .inner_string_text()
            .ok()
            .map(|inner| inner.text().to_string())
            .is_some_and(|escaped_text| !escaped_text.trim().is_empty()),
        AnyJsxAttributeValue::JsxExpressionAttributeValue(_) => true,
        _ => false,
    }
}
