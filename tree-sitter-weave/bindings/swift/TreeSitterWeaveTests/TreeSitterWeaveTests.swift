import XCTest
import SwiftTreeSitter
import TreeSitterWeave

final class TreeSitterWeaveTests: XCTestCase {
    func testCanLoadGrammar() throws {
        let parser = Parser()
        let language = Language(language: tree_sitter_weave())
        XCTAssertNoThrow(try parser.setLanguage(language),
                         "Error loading Weave grammar")
    }
}
