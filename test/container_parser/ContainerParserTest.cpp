#include <gtest/gtest.h>

#include <string>

#include "ContainerParser.h"

namespace {

struct Result {
  std::string fullPath;
  bool complete = true;  // every write took its bytes
};

Result parse(const std::string& xml, const size_t chunk = 0) {
  Result result;
  ContainerParser parser(xml.size());
  EXPECT_TRUE(parser.setup());
  const size_t step = chunk ? chunk : xml.size();
  for (size_t pos = 0; pos < xml.size(); pos += step) {
    const size_t n = std::min(step, xml.size() - pos);
    if (parser.write(reinterpret_cast<const uint8_t*>(xml.data() + pos), n) != n) result.complete = false;
  }
  result.fullPath = parser.fullPath;
  return result;
}

constexpr char kPackage[] = "application/oebps-package+xml";

std::string container(const std::string& rootfiles) {
  return R"(<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">)"
         "<rootfiles>" +
         rootfiles + "</rootfiles></container>";
}

std::string rootfile(const std::string& path, const std::string& type = kPackage) {
  return "<rootfile full-path=\"" + path + "\" media-type=\"" + type + "\"/>";
}

}  // namespace

TEST(ContainerParser, FindsThePackage) {
  const auto r = parse(container(rootfile("OEBPS/content.opf")));
  EXPECT_TRUE(r.complete);
  EXPECT_EQ(r.fullPath, "OEBPS/content.opf");
}

TEST(ContainerParser, KeepsThePathAsWrittenWithEntitiesDecoded) {
  const auto r = parse(container(rootfile("a/../b%20c/&amp;x.opf")));
  EXPECT_EQ(r.fullPath, "a/../b%20c/&x.opf");
}

TEST(ContainerParser, OnlyThePackageMediaTypeCounts) {
  EXPECT_EQ(parse(container(rootfile("x.pdf", "application/pdf"))).fullPath, "");
  EXPECT_EQ(parse(container("<rootfile full-path=\"no-type.opf\"/>")).fullPath, "");
  EXPECT_EQ(parse(container("<rootfile media-type=\"application/oebps-package+xml\"/>")).fullPath, "");
}

TEST(ContainerParser, TheLastPackageRootfileWins) {
  const auto r = parse(container(rootfile("first.opf") + rootfile("x.pdf", "application/pdf") + rootfile("second.opf")));
  EXPECT_EQ(r.fullPath, "second.opf");
}

TEST(ContainerParser, AnEmptyPathIsKept) {
  EXPECT_EQ(parse(container(rootfile("first.opf") + rootfile(""))).fullPath, "");
}

TEST(ContainerParser, RootfilesOutsideTheStructureAreIgnored) {
  EXPECT_EQ(parse("<container>" + rootfile("bare.opf") + "</container>").fullPath, "");
  EXPECT_EQ(parse("<rootfiles>" + rootfile("no-container.opf") + "</rootfiles>").fullPath, "");
  EXPECT_EQ(parse("<c:container xmlns:c=\"x\"><c:rootfiles>" + rootfile("prefixed.opf") + "</c:rootfiles></c:container>")
                .fullPath,
            "");
  // A rootfiles element nested deeper still counts once the container is open.
  EXPECT_EQ(parse("<container><x><rootfiles>" + rootfile("nested.opf") + "</rootfiles></x></container>").fullPath,
            "nested.opf");
}

TEST(ContainerParser, AfterTheContainerClosesASecondOneIsRead) {
  const std::string doc = "<root><container><rootfiles>" + rootfile("one.opf") +
                          "</rootfiles></container><container><rootfiles>" + rootfile("two.opf") +
                          "</rootfiles></container></root>";
  EXPECT_EQ(parse(doc).fullPath, "two.opf");
}

TEST(ContainerParser, ReadsTheSameInAnyPieces) {
  const std::string doc = container(rootfile("ÖEBPS/İçerik.opf") + rootfile("x.pdf", "application/pdf"));
  for (const size_t chunk : {1u, 2u, 5u, 33u}) {
    const auto r = parse(doc, chunk);
    EXPECT_TRUE(r.complete) << chunk;
    EXPECT_EQ(r.fullPath, "ÖEBPS/İçerik.opf") << chunk;
  }
}

TEST(ContainerParser, MalformedXmlStopsTheDocumentKeepingWhatWasRead) {
  const auto r = parse(container(rootfile("before.opf") + "<rootfile></rootfiles>" + rootfile("after.opf")));
  EXPECT_FALSE(r.complete);
  EXPECT_EQ(r.fullPath, "before.opf");
}

TEST(ContainerParser, AnUndeclaredEntityStopsTheDocument) {
  const auto r = parse(container(rootfile("a&nbsp;b.opf")));
  EXPECT_FALSE(r.complete);
  EXPECT_EQ(r.fullPath, "");
}
