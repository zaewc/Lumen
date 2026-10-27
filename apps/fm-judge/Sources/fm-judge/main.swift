// fm-judge: Lumen's on-device judge helper for evaluation (ADR-0044).
//
//   fm-judge            read one jev.request/1 per line on standard input and
//                       write one fm.answer/1 or fm.error/1 line per request
//   fm-judge --check    write one fm.status/1 line; exit 0 when the model is
//                       available, 3 when Apple Intelligence is not enabled,
//                       4 when the model is not ready, 5 when this Mac is not
//                       eligible, 6 when it is unavailable for another reason
//   fm-judge --render   per request line, the bytes of the instructions, the
//                       schema and the item (no model needed)
//   fm-judge --measure  per request line, exact token counts and the context
//                       size (macOS 26.4 or later, model available)
//
// Nothing but those lines is written to standard output; no model text is
// ever written anywhere. Requests are metadata only (never file contents)
// and nothing leaves the Mac: the model runs on device.
import FMJudgeCore
import Foundation
import FoundationModels

func emit(_ text: String) {
  FileHandle.standardOutput.write(Data((text + "\n").utf8))
}

func eachLine(_ body: (String) async -> String) async {
  while let text = readLine(strippingNewline: true) {
    if text.isEmpty { continue }
    emit(await body(text))
  }
}

let version = modelVersion()
let arguments = Array(CommandLine.arguments.dropFirst())
switch arguments {
case []:
  await eachLine { await Judge.answer(line: $0, modelVersion: version) }
case ["--check"]:
  let model = SystemLanguageModel.default
  let unavailable = ErrorCode(model.availability)
  emit(
    line(
      StatusLine(unavailable: unavailable, modelVersion: version, contextSize: model.contextSize)))
  if let unavailable {
    FileHandle.standardError.write(Data("fm-judge: \(unavailable.message)\n".utf8))
    exit(unavailable.exitCode)
  }
case ["--render"]:
  await eachLine { Judge.render(line: $0) }
case ["--measure"]:
  if #available(macOS 26.4, *) {
    await eachLine { await Judge.measure(line: $0) }
  } else {
    FileHandle.standardError.write(Data("fm-judge: --measure needs macOS 26.4 or later\n".utf8))
    exit(2)
  }
default:
  FileHandle.standardError.write(Data("usage: fm-judge [--check | --render | --measure]\n".utf8))
  exit(2)
}
