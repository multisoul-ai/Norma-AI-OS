"""Fixed mixed-token normalization and Levenshtein error accounting."""

from dataclasses import dataclass
import unicodedata


NORMALIZATION_VERSION = "nfkc-casefold-cjkchar-word-v1"


def is_cjk(character: str) -> bool:
    code = ord(character)
    return (0x3400 <= code <= 0x9FFF or 0xF900 <= code <= 0xFAFF
            or 0x20000 <= code <= 0x323AF)


def mixed_tokens(text: str) -> list[str]:
    """Chinese ideographs are tokens; other letters/digits form words.

    Punctuation separates words, except an internal apostrophe. No translation,
    simplified/traditional conversion, or number verbalization is performed.
    """
    text = unicodedata.normalize("NFKC", text).casefold().replace("’", "'")
    result, word = [], []
    for index, character in enumerate(text):
        if is_cjk(character):
            if word:
                result.append("".join(word))
                word = []
            result.append(character)
        elif character.isalnum():
            word.append(character)
        elif (character == "'" and word and index + 1 < len(text)
              and text[index + 1].isalnum() and not is_cjk(text[index + 1])):
            word.append(character)
        elif word:
            result.append("".join(word))
            word = []
    if word:
        result.append("".join(word))
    return result


@dataclass(frozen=True)
class ErrorCounts:
    reference_tokens: int
    substitutions: int
    deletions: int
    insertions: int

    @property
    def errors(self) -> int:
        return self.substitutions + self.deletions + self.insertions

    @property
    def error_rate(self) -> float | None:
        return self.errors / self.reference_tokens if self.reference_tokens else None


def score_pair(reference: str, hypothesis: str) -> ErrorCounts:
    ref, hyp = mixed_tokens(reference), mixed_tokens(hypothesis)
    # Cells retain (total, substitutions, deletions, insertions). Deterministic
    # ties prefer substitution, then deletion, then insertion.
    previous = [(j, 0, 0, j) for j in range(len(hyp) + 1)]
    for i, expected in enumerate(ref, 1):
        current = [(i, 0, i, 0)]
        for j, actual in enumerate(hyp, 1):
            if expected == actual:
                current.append(previous[j - 1])
            else:
                t, s, d, n = previous[j - 1]
                substitution = (t + 1, s + 1, d, n)
                t, s, d, n = previous[j]
                deletion = (t + 1, s, d + 1, n)
                t, s, d, n = current[j - 1]
                insertion = (t + 1, s, d, n + 1)
                current.append(min((substitution, deletion, insertion), key=lambda x: x[0]))
        previous = current
    _, substitutions, deletions, insertions = previous[-1]
    return ErrorCounts(len(ref), substitutions, deletions, insertions)


def aggregate_pairs(pairs) -> dict:
    counts = [score_pair(reference, hypothesis) for reference, hypothesis in pairs]
    positive = [count for count in counts if count.reference_tokens]
    negative = [count for count in counts if not count.reference_tokens]
    reference_tokens = sum(count.reference_tokens for count in positive)
    errors = sum(count.errors for count in positive)
    return {
        "normalization": NORMALIZATION_VERSION,
        "positive_samples": len(positive), "negative_samples": len(negative),
        "reference_tokens": reference_tokens, "errors": errors,
        "mer": errors / reference_tokens if reference_tokens else None,
        "false_output_rate": (sum(count.insertions > 0 for count in negative)
                              / len(negative)) if negative else None,
    }
