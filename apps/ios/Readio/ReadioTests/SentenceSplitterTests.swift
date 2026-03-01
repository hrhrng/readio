import XCTest
@testable import Readio

// ---------------------------------------------------------------------------
// MARK: - SentenceSplitter unit tests
// Validates sentence boundary detection for English, CJK, and mixed text.
// These tests ensure parity with the web app's lib/sentences.ts behavior.
// ---------------------------------------------------------------------------

final class SentenceSplitterTests: XCTestCase {

    // MARK: - English sentence splitting

    /// Standard English sentences separated by period + space.
    func testEnglishBasicSplit() {
        let text = "Hello world. This is a test. Final sentence."
        let sentences = SentenceSplitter.splitSentences(text)

        XCTAssertEqual(sentences.count, 3)
        XCTAssertEqual(sentences[0].text, "Hello world.")
        XCTAssertEqual(sentences[1].text, "This is a test.")
        XCTAssertEqual(sentences[2].text, "Final sentence.")
    }

    /// Exclamation marks and question marks also serve as boundaries.
    func testEnglishPunctuationVariety() {
        let text = "Wow! Is this real? Yes it is."
        let sentences = SentenceSplitter.splitSentences(text)

        XCTAssertEqual(sentences.count, 3)
        XCTAssertEqual(sentences[0].text, "Wow!")
        XCTAssertEqual(sentences[1].text, "Is this real?")
        XCTAssertEqual(sentences[2].text, "Yes it is.")
    }

    /// Abbreviations with periods (Mr., Dr., etc.) should not cause splits
    /// because they are not followed by a space + uppercase in typical usage.
    /// Note: our simple splitter does split on ". " — this tests current behavior.
    func testEnglishSingleSentence() {
        let text = "Just one sentence without final punctuation"
        let sentences = SentenceSplitter.splitSentences(text)

        XCTAssertEqual(sentences.count, 1)
        XCTAssertEqual(sentences[0].text, "Just one sentence without final punctuation")
    }

    /// Multiple spaces between sentences should be handled.
    func testEnglishMultipleSpaces() {
        let text = "First.  Second.   Third."
        let sentences = SentenceSplitter.splitSentences(text)

        XCTAssertEqual(sentences.count, 3)
    }

    // MARK: - CJK sentence splitting

    /// Chinese sentences split on 。(fullwidth period).
    func testChineseSentences() {
        let text = "这是第一句。这是第二句。这是第三句。"
        let sentences = SentenceSplitter.splitSentences(text)

        XCTAssertEqual(sentences.count, 3)
        XCTAssertEqual(sentences[0].text, "这是第一句。")
        XCTAssertEqual(sentences[1].text, "这是第二句。")
        XCTAssertEqual(sentences[2].text, "这是第三句。")
    }

    /// Chinese exclamation and question marks (fullwidth) split sentences.
    func testChinesePunctuationVariety() {
        let text = "你好！今天怎么样？还不错。"
        let sentences = SentenceSplitter.splitSentences(text)

        XCTAssertEqual(sentences.count, 3)
        XCTAssertEqual(sentences[0].text, "你好！")
        XCTAssertEqual(sentences[1].text, "今天怎么样？")
        XCTAssertEqual(sentences[2].text, "还不错。")
    }

    /// CJK text without sentence-ending punctuation stays as one sentence.
    func testChineseSingleSentence() {
        let text = "这是一个没有句号的句子"
        let sentences = SentenceSplitter.splitSentences(text)

        XCTAssertEqual(sentences.count, 1)
    }

    // MARK: - Mixed language splitting

    /// Mixed English and Chinese in one text block.
    func testMixedLanguage() {
        let text = "Hello world. 你好世界。Goodbye."
        let sentences = SentenceSplitter.splitSentences(text)

        XCTAssertEqual(sentences.count, 3)
        XCTAssertEqual(sentences[0].text, "Hello world.")
        XCTAssertEqual(sentences[1].text, "你好世界。")
        XCTAssertEqual(sentences[2].text, "Goodbye.")
    }

    // MARK: - Edge cases

    /// Empty string should produce no sentences.
    func testEmptyString() {
        let sentences = SentenceSplitter.splitSentences("")
        XCTAssertTrue(sentences.isEmpty)
    }

    /// Whitespace-only string should produce no sentences.
    func testWhitespaceOnly() {
        let sentences = SentenceSplitter.splitSentences("   \n\t  ")
        XCTAssertTrue(sentences.isEmpty)
    }

    /// Sentence indices should be sequential starting from 0.
    func testSequentialIndices() {
        let text = "One. Two. Three."
        let sentences = SentenceSplitter.splitSentences(text)

        for (i, sentence) in sentences.enumerated() {
            XCTAssertEqual(sentence.index, i, "Sentence at position \(i) has wrong index")
        }
    }

    // MARK: - CJK detection

    func testIsCJK() {
        // Chinese characters
        XCTAssertTrue(SentenceSplitter.isCJK(Character("中")))
        XCTAssertTrue(SentenceSplitter.isCJK(Character("国")))

        // Japanese Hiragana and Katakana
        XCTAssertTrue(SentenceSplitter.isCJK(Character("あ")))
        XCTAssertTrue(SentenceSplitter.isCJK(Character("ア")))

        // Korean Hangul
        XCTAssertTrue(SentenceSplitter.isCJK(Character("한")))

        // Latin characters are NOT CJK
        XCTAssertFalse(SentenceSplitter.isCJK(Character("A")))
        XCTAssertFalse(SentenceSplitter.isCJK(Character("z")))
        XCTAssertFalse(SentenceSplitter.isCJK(Character("1")))
    }

    // MARK: - Token counting

    /// English words count as one token each, spaces don't count.
    func testCountTokensEnglish() {
        XCTAssertEqual(SentenceSplitter.countTokens("Hello world"), 2)
        XCTAssertEqual(SentenceSplitter.countTokens("One"), 1)
        XCTAssertEqual(SentenceSplitter.countTokens("A B C D"), 4)
    }

    /// CJK characters each count as one token.
    func testCountTokensCJK() {
        XCTAssertEqual(SentenceSplitter.countTokens("你好世界"), 4)
        XCTAssertEqual(SentenceSplitter.countTokens("中"), 1)
    }

    /// Mixed text: CJK chars + Latin words, each counts as a token.
    func testCountTokensMixed() {
        // "Hello" = 1 token, " " = skip, "你好" = 2 tokens
        XCTAssertEqual(SentenceSplitter.countTokens("Hello 你好"), 3)
    }

    /// Empty string has zero tokens.
    func testCountTokensEmpty() {
        XCTAssertEqual(SentenceSplitter.countTokens(""), 0)
    }

    // MARK: - Tokenization

    /// Tokenize English text into words and whitespace.
    func testTokenizeEnglish() {
        let tokens = SentenceSplitter.tokenize("Hello world")
        XCTAssertEqual(tokens, ["Hello", " ", "world"])
    }

    /// Tokenize CJK text into individual characters.
    func testTokenizeCJK() {
        let tokens = SentenceSplitter.tokenize("你好世界")
        XCTAssertEqual(tokens, ["你", "好", "世", "界"])
    }

    /// Tokenize mixed text preserving both CJK chars and Latin words.
    func testTokenizeMixed() {
        let tokens = SentenceSplitter.tokenize("Hi 你好")
        XCTAssertEqual(tokens, ["Hi", " ", "你", "好"])
    }
}
