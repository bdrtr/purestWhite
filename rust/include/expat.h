// The part of expat's API the FreeInk SDK uses (libs/network/Opds and CatalogList leave the XML
// library to the firmware), implemented by rust/crates/bembeyaz-ffi/src/expat.rs over the
// firmware's XML parser. Firmware code uses lib/XmlParserUtils/XmlParser.h instead.
//
// Unlike expat: XML_ParserCreate takes no encoding (it returns NULL for one), XML_StopParser
// always stops for good, handlers and user data set during a parse apply from the next parse call, and
// XML_GetCurrentLineNumber returns 0.
#pragma once

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#define XMLCALL
#define XML_TRUE ((XML_Bool)1)
#define XML_FALSE ((XML_Bool)0)

typedef char XML_Char;
typedef char XML_LChar;
typedef unsigned char XML_Bool;
typedef unsigned long XML_Size;
typedef struct XML_ParserStruct* XML_Parser;

enum XML_Status { XML_STATUS_ERROR = 0, XML_STATUS_OK = 1, XML_STATUS_SUSPENDED = 2 };

// The codes the parser reports, numbered as in expat.
enum XML_Error {
  XML_ERROR_NONE = 0,
  XML_ERROR_NO_MEMORY = 1,
  XML_ERROR_SYNTAX = 2,
  XML_ERROR_NO_ELEMENTS = 3,
  XML_ERROR_INVALID_TOKEN = 4,
  XML_ERROR_UNCLOSED_TOKEN = 5,
  XML_ERROR_PARTIAL_CHAR = 6,
  XML_ERROR_TAG_MISMATCH = 7,
  XML_ERROR_DUPLICATE_ATTRIBUTE = 8,
  XML_ERROR_JUNK_AFTER_DOC_ELEMENT = 9,
  XML_ERROR_PARAM_ENTITY_REF = 10,
  XML_ERROR_UNDEFINED_ENTITY = 11,
  XML_ERROR_BAD_CHAR_REF = 14,
  XML_ERROR_BINARY_ENTITY_REF = 15,
  XML_ERROR_MISPLACED_XML_PI = 17,
  XML_ERROR_UNKNOWN_ENCODING = 18,
  XML_ERROR_INCORRECT_ENCODING = 19,
  XML_ERROR_UNCLOSED_CDATA_SECTION = 20,
  XML_ERROR_XML_DECL = 30,
  XML_ERROR_PUBLICID = 32,
  XML_ERROR_ABORTED = 35,
  XML_ERROR_FINISHED = 36,
  XML_ERROR_INVALID_ARGUMENT = 41,
};

typedef void(XMLCALL* XML_StartElementHandler)(void* userData, const XML_Char* name, const XML_Char** atts);
typedef void(XMLCALL* XML_EndElementHandler)(void* userData, const XML_Char* name);
typedef void(XMLCALL* XML_CharacterDataHandler)(void* userData, const XML_Char* s, int len);

XML_Parser XML_ParserCreate(const XML_Char* encoding);
void XML_ParserFree(XML_Parser parser);
void XML_SetUserData(XML_Parser parser, void* userData);
void XML_SetElementHandler(XML_Parser parser, XML_StartElementHandler start, XML_EndElementHandler end);
void XML_SetCharacterDataHandler(XML_Parser parser, XML_CharacterDataHandler handler);
enum XML_Status XML_Parse(XML_Parser parser, const char* s, int len, int isFinal);
void* XML_GetBuffer(XML_Parser parser, int len);
enum XML_Status XML_ParseBuffer(XML_Parser parser, int len, int isFinal);
enum XML_Status XML_StopParser(XML_Parser parser, XML_Bool resumable);
enum XML_Error XML_GetErrorCode(XML_Parser parser);
const XML_LChar* XML_ErrorString(enum XML_Error code);
XML_Size XML_GetCurrentLineNumber(XML_Parser parser);

#ifdef __cplusplus
}
#endif
