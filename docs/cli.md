# CLI·설치·설정·엔진 검사와 초기화

`--json`은 명령 인자 앞/뒤에서 받을 수 있으며 stdout에 JSON 문서 하나만 쓴다. setup에서는 `--non-interactive`와 함께 사용해야 한다. 사람용 결과도 같은 내부 결과를 표현한다. 대화형 setup 질문은 stderr에 출력하며, 비대화형 JSON 실행은 질문이나 로그 없이 결과만 stdout에 쓴다. 잘못된 인자나 parser 오류에 입력값·secret을 그대로 되돌려 쓰지 않는다.

| exit | 현재 의미 |
| --- | --- |
| 0 | 패키지 검증·파일 설치·엔진 식별·설정 저장·인스턴스 등록/조회·초기화 성공. 노드 정상 보장이 아님 |
| 2 | 인자 오류, `config validate`/로컬 전용 `preflight`의 설정 read/schema 오류 |
| 3 | artifact·설치·engine lock/응답/입력 검증 실패, 결합 preflight 불일치, setup 저장 오류 또는 등록 인스턴스의 잠금·입력 변경·초기화 사전 조건 거부 |
| 4 | 로컬 preflight FAIL 또는 아직 제공하지 않는 운영 명령 |
| 5 | preflight 미완료 또는 setup 취소/EOF·비대화형 미완성 초안 |
| 6 | cold JVM timeout, 설치 receipt commit 불명 또는 초기화 시도/결과 기록 불명(UNKNOWN) |
| 7 | 입력/안내·결과 출력 실패 또는 분류되지 않은 내부 오류 |

engine 명령과 결합/등록형 preflight는 고정한 JVM의 cold 명령을 실행한다. cold timeout은 직접 child를 종료·회수하고 UNKNOWN/exit 6을 반환한다. 쓰기 작업인 init/resume-init은 TERM 후 최대 5초를 더 기다리며 SIGKILL하지 않는다. child가 살아 있거나 잠금을 보유할 수 있으므로 UNKNOWN을 종료 완료로 해석하지 않는다. 두 경로 모두 자동 재시도하지 않는다.

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

validate는 config 자체만 읽고 참조 파일을 열지 않는다. 엔진 옵션 없는 preflight는 파일/디렉터리의 type·기본 mode·symlink 등 metadata만 관측한다. secret 파일 내용·인증서·DB를 열지 않고 포트를 bind/connect하지 않으며 디렉터리를 생성하거나 프로그램을 실행하지 않는다. config 읽기에 따라 filesystem atime이 달라질 수 있다.

secret 파일의 기본 metadata 허용은 0600/0640 수준, parent는 0700/0750 수준이다. 서비스 UID·ACL·mount·effective access는 해당 Mac/Linux 설치 profile에서 검증할 항목이며 이 metadata 검사로 보증하지 않는다. setup이 새로 저장하는 초안·설정은 0600이다. 누락 data directory는 DATA_NOT_INITIALIZED/NOT_CHECKED로 표시한다.

로컬 보고서에서 chain/genesis/profile·key identity·인증서 유효성·DB/WAL·네트워크·JRE/native·Mac launchd/Linux systemd·instance 등록은 NOT_CHECKED다. 엔진 옵션 없는 `preflight` 명령은 모든 로컬 metadata가 PASS라도 INCOMPLETE와 exit 5이며 `ready`를 반환하지 않는다. 로컬 문제가 있으면 FAIL과 exit 4로 상세 checks를 유지한다. setup은 위에서 설명한 저장 요청의 exit 규칙을 사용한다.

## 제품·native 설정 결합 preflight

```text
bxdl preflight --config <instance.json> --engine-config <node.json>
    --jar <jar> --java <absolute-java> --lock <trusted-engine.lock.json>
    --allow-development [--timeout-seconds <1..120>] [--json]
```

`--config`는 제품 설정, `--engine-config`는 운영자가 준비한 NIGO native 설정이다. 다섯 엔진 옵션 `--engine-config/--jar/--java/--lock/--allow-development`는 전부 함께 요구한다. `--timeout-seconds`만 붙이거나 일부 옵션을 빠뜨리면 exit 2다. 엔진 옵션이 없으면 기존 로컬 metadata 검사와 결과 구조를 유지한다. 제한 시간은 기본 30초이며 engine-info와 preflight 각 JVM 호출에 적용한다.

제품의 14개 입력 중 instanceId는 제품 전용이다. 나머지 입력 및 validator/RocksDB 고정값이 명시 native 값과 일치해야 한다. native는 QBFT·VALIDATOR·MTLS를 명시하며, 대응 값의 누락을 엔진 기본값으로 채우지 않는다. nodeId·경로·주소·포트·secret 참조를 비교한 뒤 pinned 엔진의 cold 검사를 수행한다. 제품에 없는 validator ID·peer/pin·sync 등의 내용은 native로 준비하며 NIGO가 검증한다. [상세 매핑](../design/2026-09-18-product-engine-preflight.md)과 [엔진 가이드](./engine.md)를 따른다.

결합 보고서 `data`에는 `outcome=FAIL|INCOMPLETE`, `product`(기존 로컬 report), `configurationBinding=NOT_CHECKED|MATCHED`, 선택 `engine`(기존 engine report)이 있다. 로컬 전용 명령의 `data`는 기존 report 자체이며 이 새 구조로 감싸지 않는다.

| 조건 | envelope / exit / 결과 |
| --- | --- |
| 제품 참조 metadata에 FAIL | FAILED / 4 / `data.product`에 실패 보존, binding NOT_CHECKED, engine 없음; JVM 미실행 |
| 실행 전 제품/native 대응 불일치·필수 대응 값 누락 | FAILED / 3 / ENGINE_PRODUCT_MISMATCH; JVM 미실행 |
| 결합 입력 read/schema·lock/pin·응답 검증 오류 | FAILED / 3 / 정제된 reasonCode; 성공 보고서 없음 |
| 일치 확인 및 정상 cold 결과 | INCOMPLETE / 5 / binding MATCHED, engine 결과 포함 |
| cold JVM timeout | UNKNOWN / 6 / ENGINE_TIMEOUT; READY로 해석하거나 자동 재시도하지 않음 |

실행한 엔진 응답의 node identity/backend가 제품과 달라도 ENGINE_PRODUCT_MISMATCH로 결과를 거부한다. 따라서 reasonCode만으로 JVM 미실행 여부를 추론하지 않는다. 비교·실행한 product/native/chain bytes를 결속하고 원본 변경 시 결과를 폐기한다. `data.product`의 로컬 미검사는 엔진 결과로 덮어쓰지 않는다. NIGO cold는 key material을 읽지만 DB open·서명·network·서비스 시작을 수행하지 않는다. KEY_MATERIAL PASS도 peer handshake·expiry/revocation 인수가 아니다. 설정 원문·secret 참조 경로·child 원문 출력은 envelope에 넣지 않는다.

이 명령은 설정 자동 렌더·v1 migration·PKI 생성·설치 등록·init·launchd를 추가하지 않는다. setup은 계속 초안과 제품 JSON만 저장하며, 설치 receipt나 cold 결과를 실행 준비 완료로 승격하지 않는다.

## 파일 설치와 엔진 cold 검사

`install`은 명시한 새 폴더에만 설치한다. 외부 신뢰 key 또는 서명 없는 개발 자료 opt-in이 필요하다. `engine inspect`/`engine preflight`는 `--jar`, `--java`, `--lock`, `--allow-development`가 필수다. preflight에는 NIGO `--config`도 필요하다. 선택 `--timeout-seconds`는 1~120, 기본 30이며 각 JVM 호출에 적용한다. [설치](./install.md)와 [엔진](./engine.md) 가이드의 신뢰·실패 경계를 따른다.

## 등록 인스턴스와 명시 초기화

```text
bxdl instance register --instance <new-control-dir> --package <installed-dir>
    --archive <tar.gz> (--public-key <trusted.pem> | --allow-unsigned-development)
    --config <instance.json> --engine-config <node.json> --lock <trusted-lock.json>
    --allow-development [--timeout-seconds <1..120>] [--json]
bxdl instance show --instance <control-dir> [--json]
bxdl preflight --instance <control-dir> [--timeout-seconds <1..120>] [--json]
bxdl init --instance <control-dir> --confirm-initialize
    [--timeout-seconds <1..600>] [--json]
bxdl resume-init --instance <control-dir> --confirm-resume
    [--timeout-seconds <1..600>] [--json]
```

`--instance`는 instanceId가 아닌 제어 폴더 경로다. 절대경로를 사용하며 등록 대상은 새 폴더여야 한다. 등록/초기 운용은 macOS arm64 개발 profile을 대상으로 한다. package의 `engine/nigo-node.jar`와 `runtime/bin/java`를 사용하므로 후속 호출에는 JAR/Java를 지정하지 않는다. `--instance` 방식은 기존 preflight의 제품/native/engine 옵션과 섞지 않는다.

register는 필수 옵션 전부와 `--public-key` 또는 `--allow-unsigned-development` 중 하나를 요구한다. 원본 archive·신뢰 key·engine lock은 설치 package 밖에 있어야 한다. 개발용 선택은 등록에 저장되며 init/resume에는 작업별 확인 flag만 추가한다. archive/key·설정·chain·참조 credential은 이후에도 원래 위치와 내용으로 필요하다. 등록은 private credential 내용의 hash도 private binding에 고정하며 원문/hash/secret 경로를 공개 Summary에 표시하지 않는다.

등록 전과 등록 후 작업에서 archive 신뢰 검증·전체 설치 manifest 검증·제품/native 대응·고정 engine identity를 확인한다. 등록에는 없거나 빈 data와 존재하는 부모가 필요하며 기존/외부 DB를 채택하지 않는다. 등록 성공은 INSTANCE_REGISTERED/exit 0이며 데이터는 초기화하지 않는다. 등록 preflight는 정상 cold라도 INCOMPLETE/exit 5다. 이 경로의 입력 변경·local 조건 미충족·잠금 중은 exit 3이며 local 전용 `preflight --config`의 FAIL/exit 4와 구분한다.

register/preflight timeout은 기본 30초, 최대 120초이며 각 cold JVM 호출에 적용한다. init/resume timeout은 기본 120초, 최대 600초다. 초기화 전 cold는 각각 최대 120초이며 전체 명령의 총 시간 제한이 아니다. 새 init은 등록 직후의 없거나 0700인 빈 data만 받는다. intent를 내구성 있게 기록한 뒤 데이터를 준비하고 NIGO를 호출한다. resume는 BXDL 미완료 상태 및 같은 identity의 INITIALIZING 엔진 journal과 기존 ledger가 있어야 한다.

init/resume 성공은 child 종료·exit 0, stdout·동일 attempt/PID report·engine-instance.json과 genesis/identity 대조, 입력·설치본 재검사와 결과 저장까지 확인한 INSTANCE_INITIALIZED/exit 0이다. intent 후 불명 결과는 INSTANCE_INITIALIZATION_UNKNOWN/UNKNOWN/exit 6이며 시도와 데이터를 보존한다. timeout 시 TERM 후 5초 대기하고 SIGKILL하지 않는다. CLI crash 뒤 남은 INITIALIZED 엔진 기록을 자동 채택하지 않으며 이미 초기화된 인스턴스의 init/resume 반복은 거부한다.

register/show/init/resume의 `data`는 `instanceId`, `initialization=NOT_STARTED|INITIALIZED|UNKNOWN`, 선택 `operationBusy`, `developmentOnly=true`, `serviceRegistration=NOT_REGISTERED`, `runtimeReadiness=NOT_CHECKED`, `reason`, 선택 `attemptId/genesisHash`를 포함한다. show의 exit 0은 조회 성공이며 operationBusy는 순간적인 advisory 관측이다. 노드 health나 정상 종료의 증명이 아니다. 불명 init 결과에는 busy를 추측해 넣지 않는다. 등록 preflight의 data는 기존 ProductReport다.

[인스턴스 가이드](./instance.md)에 준비 자료, 중단 후 명시 재개 조건, 보존된 시도, 이동/복원 미지원 경계를 정리한다. 원문 설정·secret 경로·child 출력은 결과에 노출하지 않는다. 서비스 등록·시작은 수행하지 않는다.

## 미구현 명령

start/stop/status/logs/diagnose/upgrade/uninstall은 명시적인 UNSUPPORTED 오류를 반환한다. no-op 성공이나 가짜 PID/engine 상태를 만들지 않는다. setup의 패키지 선택·설치 연결, launchd/systemd 설치·실행과 인증된 원격 관리는 아직 제공하지 않는다. macOS를 첫 UX 대상으로 구현할 순서는 [Mac 우선 설계](../design/2026-09-17-rust-macos-first.md)를 따른다.
