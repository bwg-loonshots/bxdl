# CLI와 로컬 설정 검사

`--json`은 명령 인자 앞/뒤에서 받을 수 있으며 stdout에 JSON 문서 하나만 쓴다. 사람용 결과도 같은 내부 결과를 표현한다. 잘못된 인자나 parser 오류에 입력값·secret을 그대로 되돌려 쓰지 않는다.

| exit | 현재 의미 |
| --- | --- |
| 0 | 요청 처리 성공. 패키지 내용 또는 제품 schema 검증이며 엔진 정상 보장이 아님 |
| 2 | 인자·제품 설정 read/schema 오류 |
| 3 | artifact·서명·stage·expected hash·형식/한도 오류 |
| 4 | 로컬 preflight FAIL 또는 아직 제공하지 않는 운영 명령 |
| 5 | 로컬 검사 후 engine/platform/effective access 검사 미완료 |
| 7 | 출력 실패 또는 분류되지 않은 내부 오류 |

설계의 exit 6(timeout/결과 불명)은 서비스 관리 도입 시 사용한다. 현재는 외부 작업을 시작하지 않아 해당 성공/취소 의미를 제공하지 않는다.

## JSON envelope

`schemaVersion`, `command`, `outcome`, `reasonCode`, `message`, `observedAt`, 선택 `data`로 구성된다. outcome은 SUCCEEDED/FAILED/INCOMPLETE/UNSUPPORTED다. Rust 이식 후에도 명령·exit·envelope를 유지한다. `version`은 CLI build와 capability만 표시하며 설치된 bundle을 탐색하지 않는다. data에 implementation=rust, primaryTarget=darwin-arm64, macosServiceAcceptance=NOT_CHECKED를 추가하고 기존 linuxServiceAcceptance도 유지한다.

## 제품 설정 v1

현재는 JSON을 사용하며 schema는 `contracts/bxdl/instance.schema.json`이다. 설계의 YAML 입력과 NIGO YAML rendering은 canonical mapping 계약이 제공되면 추가한다. NIGO 설정을 추측 생성하지 않는다.

- schemaVersion 1, validator role, RocksDB backend를 명시한다.
- HTTP는 127.0.0.1 또는 ::1, P2P는 명시한 unicast IP다. 두 port는 1~65535 범위의 서로 다른 값이다.
- instanceId는 영문 소문자로 시작하는 1~32자의 소문자·숫자·하이픈이다.
- 공통 chain description, data directory, signer/TLS material은 파일 경로 참조로 제공한다. password 원문을 넣는 field는 없다.
- 상대경로는 config 파일 디렉터리 기준이다. cwd의 영향 없이 정규화한다. 출력에는 secret path나 설정 원문을 포함하지 않는다.
- unknown field·duplicate key·case alias·불명 schema·암묵적 개발 기본값을 거부한다.

```bash
bxdl config validate --file ./instance.json --json
bxdl preflight --config ./instance.json --json
```

validate는 config 자체만 읽고 참조 파일을 열지 않는다. preflight는 파일/디렉터리의 type·기본 mode·symlink 등 metadata만 관측한다. secret 파일 내용·인증서·DB를 열지 않고 포트를 bind/connect하지 않으며 디렉터리를 생성하거나 프로그램을 실행하지 않는다. config 읽기에 따라 filesystem atime이 달라질 수 있다.

secret 파일의 기본 metadata 허용은 0600/0640 수준, parent는 0700/0750 수준이다. 서비스 UID·ACL·mount·effective access는 실제 Linux 설치 시 검증할 항목이며 이 metadata 검사로 보증하지 않는다. 누락 data directory는 DATA_NOT_INITIALIZED/NOT_CHECKED로 표시한다.

chain/genesis/profile·key identity·인증서 유효성·DB/WAL·네트워크·JRE/native·Linux/systemd·instance 등록은 현재 NOT_CHECKED다. 모든 로컬 metadata가 PASS라도 결과는 INCOMPLETE와 exit 5이며 `ready`를 반환하지 않는다. 로컬 문제가 있으면 FAIL과 exit 4로 상세 checks를 유지한다.

## 미구현 명령

install/init/start/stop/status/logs/diagnose/upgrade/uninstall은 명시적인 UNSUPPORTED 오류를 반환한다. no-op 성공이나 가짜 PID/engine 상태를 만들지 않는다. 다음 Mac 대화형 setup도 아직 미구현이다. 현재에는 launchd/systemd 설치·실행 기능이나 인증된 원격 관리 기능이 없다. macOS를 첫 UX 대상으로 구현할 순서는 [Mac 우선 설계](../design/2026-09-17-rust-macos-first.md)를 따른다.
