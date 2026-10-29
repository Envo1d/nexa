import { i18n } from "./index";
import en from "./en.json";

const entries = Object.entries(en).filter(([, message]) => message.length > 0);

const exact = new Map(entries.map(([key, message]) => [message, key]));

function escapePattern(text: string) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace(/\s+/g, "\\s+");
}

const patterns = entries
  .filter(([, message]) => /\{\w+\}/.test(message))
  .map(([key, message]) => {
    const fields = [...message.matchAll(/\{(\w+)\}/g)].map(
      (match) => match[1]!,
    );

    const parts = message.split(/\{\w+\}/);

    const pattern = parts.map(escapePattern).join("([\\s\\S]*?)");

    return {
      key,
      fields,
      expression: new RegExp(`^${pattern}$`),
      specificity: parts.join("").length,
    };
  })
  .sort((left, right) => right.specificity - left.specificity);

const nestedFields = new Set([
  "error",
  "detail",
  "status",
  "notice",
  "category",
  "action",
  "section",
]);

/** Translate only presentation messages; canonical actions and user values stay untouched. */
export function translateUi(message: string, depth = 0): string {
  if (i18n.global.locale.value === "en" || !message || depth > 4)
    return message;

  const text = message.trim();

  const leading = message.match(/^\s*/)?.[0] ?? "";
  const trailing = message.match(/\s*$/)?.[0] ?? "";
  const key = exact.get(text) ?? exact.get(text.replace(/\s+/g, " "));

  if (key) return `${leading}${i18n.global.t(key)}${trailing}`;

  for (const pattern of patterns) {
    const match = pattern.expression.exec(text);

    if (!match) continue;

    const values = Object.fromEntries(
      pattern.fields.map((field, index) => {
        const value = match[index + 1] ?? "";

        return [
          field,
          nestedFields.has(field) ? translateUi(value, depth + 1) : value,
        ];
      }),
    );

    return `${leading}${i18n.global.t(pattern.key, values)}${trailing}`;
  }

  // Unknown OS/provider diagnostics retain their original detail.
  return message;
}
