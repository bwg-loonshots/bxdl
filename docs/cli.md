# CLI·설치·설정·엔진 검사

`--json`은 명령 인자 앞/뒤에서 받을 수 있으며 stdout에 JSON 문서 하나만 쓴다. setup에서는 `--non-interactive`와 함께 사용해야 한다. 사람용 결과도 같은 내부 결과를 표현한다. 대화형 setup 질문은 stderr에 출력하며, 비대화형 JSON 실행은 질문이나 로그 없이 결과만 stdout에 쓴다. 잘못된 인자나 parser 오류에 입력값·secret을 그대로 되돌려 쓰지 않는다.

| exit | 현재 의미 |
| --- | --- |
| 0 | 패키지 검증·파일 설치·개발 엔진 식별 확인·설정 저장 성공. 노드 정상 보장이 아님 |
| 2 | 인자 오류, `config validate`/`preflight`의 설정 read/schema 오류 |
| 3 | artifact·설치·engine lock/응답/입력 검증 실패 또는 setup 가져오기·저장·충돌 오류 |
| 4 | 로컬 preflight FAIL 또는 아직 제공하지 않는 운영 명령 |
| 5 | preflight 미완료 또는 setup 취소/EOF·비대화형 미완성 초안 |
| 6 | cold JVM timeout 또는 설치 receipt commit 결과 불명(UNKNOWN) |
| 7 | 입력/안내·결과 출력 실패 또는 분류되지 않은 내부 오류 |

engine 명령만 고정한 JVM의 cold 명령을 실행한다. timeout 때 해당 직접 child를 종료·회수하고 UNKNOWN/exit 6을 반환한다. 노드·서비스 종료의 정상성을 뜻하지 않으며 자동 재시도하지 않는다.

## JSON envelope

`schemaVersion`, `command`, `outcome`, `reasonCode`, `message`, `observedAt`, 선택 `data`로 구성된다. outcome은 SUCCEEDED/FAILED/INCOMPLETE/UNSUPPORTED/UNKNOWN이다. Rust 이식 후에도 명령·exit·envelope를 유지한다. `version`은 CLI build와 capability만 표시하며 설치된 bundle을 탐색하지 않는다. data에 implementation=rust, primaryTarget=darwin-arm64, macosServiceAcceptance=NOT_CHECKED를 추가하고 기존 linuxServiceAcceptance도 유지한다.

## setup 명령

```text
bxdl setup [--workspace <dir>] [--resume | --from <instance.json>]
    [--output <new-instance.json>]
bxdl setup [--workspace <dir>] (--from <instance.json> | --resume)
    --non-interactive [--output <new-instance.json>] [--json]
```

대화형 setup은 터미널에서 14개 제품 설정 항목을 입력받아 항목마다 저장한다. 현재 role은 validator, backend는 RocksDB다. Mac 기본 작업 폴더는 `$HOME/Library/Application Support/BXDL/setup`이며 다른 개발 플랫폼에서는 `--workspace`가 필요하다. 새 실행은 새 폴더를 만들고 기존 초안은 명시 `--resume`으로 연다. `--from`과 `--resume`은 배타적이며 비대화형에는 둘 중 하나가 필수다. `--json`만 붙여 대화형을 자동화할 수 없다.

완료 시 save/export/edit/check를 고를 수 있다. 대화형 export는 구체적 대상에 y/yes 확인을 받는다. 비대화형은 `--output`이 있으면 새 설정을 쓰고 없으면 초안만 저장한다. 기존 출력은 덮어쓰지 않으며 작업 폴더 안 출력은 `instance.json`만 허용한다. `:cancel`·EOF는 exit 5로 저장 상태를 보존한다. 가져온 설정의 상대 참조는 원본 위치에서 절대화하고 직접 입력한 상대경로는 최초 실행 디렉터리 기준으로 고정한다. [setup 사용 가이드](./setup.md)에 전체 입력·재개·경로 규칙이 있다.

setup `data`에는 `workspace`, `completedFields`, `nextField`, `totalFields`, `draftComplete`, `installation=NOT_PERFORMED`, `engineValidation=NOT_CHECKED`가 있다. 완성된 초안에는 `preflight`, 설정 출력 후에는 `configPath`가 추가된다. 입력 답·secret 참조 원문은 결과에 포함하지 않는다.

`SETUP_DRAFT_SAVED`·`SETUP_CONFIG_WRITTEN`은 요청한 저장 성공으로 exit 0/SUCCEEDED를 반환한다. 참조 누락 등은 `data.preflight.outcome=FAIL`과 개별 checks에 보존되며 저장 성공으로 덮어쓰지 않는다. 로컬 검사 실패가 없어도 NIGO·runtime·service 미검사로 이 내부 preflight는 INCOMPLETE다. 초안 검사 시 configMetadata는 `DRAFT_NOT_WRITTEN/NOT_CHECKED`, export 후 검사는 실제 출력 config를 기준으로 한다. `SETUP_PAUSED`·`SETUP_DRAFT_INCOMPLETE`는 exit 5/INCOMPLETE다.

초안 작업 폴더·설정 출력만 만들며 DB·참조 키 파일을 열거나 생성하지 않는다. setup 초안 이력은 설치·초기화 journal이나 엔진 계약이 아니다.

## 제품 설정 v1

현재는 JSON을 사용하며 schema는 `contracts/bxdl/instance.schema.json`이다. NIGO는 별도 native node/chain JSON 계약을 제공했다. 제품 schema에는 아직 peer/pin/validator 상세 자료가 없으므로 NIGO 설정을 추측 생성하지 않는다. `engine preflight --config`는 이 제품 JSON이 아닌 NIGO node.json을 받는다.

- schemaVersion 1, validator role, RocksDB backend를 명시한다.
- HTTP는 127.0.0.1 또는 ::1, P2P는 명시한 unicast IP다. 두 port는 1~65535 범위의 서로 다른 값이다.
- instanceId는 영문 소문자로 시작하는 1~32자의 소문자·숫자·하이픈이다.
- 공통 chain description, data directory, signer/TLS material은 파일 경로 참조로 제공한다. password 원문을 넣는 field는 없다.
- 상대경로는 config 파일 디렉터리 기준이다. cwd의 영향 없이 정규화한다. 명령 결과(stdout/stderr)에는 secret path나 설정 원문을 포함하지 않는다. setup의 명시적 config 파일 출력에는 필요한 참조 경로가 저장된다.
- unknown field·duplicate key·case alias·불명 schema·암묵적 개발 기본값을 거부한다.

```bash
bxdl config validate --file ./instance.json --json
bxdl preflight --config ./instance.json --json
```

validate는 config 자체만 읽고 참조 파일을 열지 않는다. preflight는 파일/디렉터리의 type·기본 mode·symlink 등 metadata만 관측한다. secret 파일 내용·인증서·DB를 열지 않고 포트를 bind/connect하지 않으며 디렉터리를 생성하거나 프로그램을 실행하지 않는다. config 읽기에 따라 filesystem atime이 달라질 수 있다.

secret 파일의 기본 metadata 허용은 0600/0640 수준, parent는 0700/0750 수준이다. 서비스 UID·ACL·mount·effective access는 해당 Mac/Linux 설치 profile에서 검증할 항목이며 이 metadata 검사로 보증하지 않는다. setup이 새로 저장하는 초안·설정은 0600이다. 누락 data directory는 DATA_NOT_INITIALIZED/NOT_CHECKED로 표시한다.

chain/genesis/profile·key identity·인증서 유효성·DB/WAL·네트워크·JRE/native·Mac launchd/Linux systemd·instance 등록은 현재 NOT_CHECKED다. 독립 `preflight` 명령은 모든 로컬 metadata가 PASS라도 INCOMPLETE와 exit 5이며 `ready`를 반환하지 않는다. 로컬 문제가 있으면 FAIL과 exit 4로 상세 checks를 유지한다. setup은 위에서 설명한 저장 요청의 exit 규칙을 사용한다.

## 파일 설치와 엔진 cold 검사

`install`은 명시한 새 폴더에만 설치한다. 외부 신뢰 key 또는 서명 없는 개발 자료 opt-in이 필요하다. `engine inspect`/`engine preflight`는 `--jar`, `--java`, `--lock`, `--allow-development`가 필수다. preflight에는 NIGO `--config`도 필요하다. 선택 `--timeout-seconds`는 1~120, 기본 30이며 각 JVM 호출에 적용한다. [설치](./install.md)와 [엔진](./engine.md) 가이드의 신뢰·실패 경계를 따른다.

## 미구현 명령

init/start/stop/status/logs/diagnose/upgrade/uninstall은 명시적인 UNSUPPORTED 오류를 반환한다. no-op 성공이나 가짜 PID/engine 상태를 만들지 않는다. setup의 패키지 선택·설치 연결, launchd/systemd 설치·실행과 인증된 원격 관리는 아직 제공하지 않는다. macOS를 첫 UX 대상으로 구현할 순서는 [Mac 우선 설계](../design/2026-09-17-rust-macos-first.md)를 따른다.
