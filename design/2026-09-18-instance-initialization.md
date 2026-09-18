# 등록 인스턴스와 명시 초기화

- 작성: 2026-09-18
- 제품 기반: BXDL `d55b584b48a4fa869526d5cbe94609c4913a92fa` 이후
- 공급 기준: [clean 개발 후보](../contracts/nigo/development-clean-2026-09-18/README.md). 문서 checkout #119 `49d1cefcfb200f8eb04b6f9961c889da21f81018`, JAR source `303e163a9b3f293fa39e42d02b8daa1843973c14`, dirty=false
- 계약 수준: PROPOSED/development/officialRelease=false, REQ-0002 OPEN 유지. 공유 NIGO checkout·요구 원장·공급자 계약을 변경하지 않는다.

## 1. 해결하는 흐름과 범위

기존 setup → 파일 install → 제품/native 결합 preflight 사이에는 운영자가 같은 경로와 신뢰 입력을 반복해서 지정해야 했다. 이제 설치본·원본 archive·신뢰 key·제품/native 설정·engine lock을 한 제어 폴더에 등록하고, 그 기록으로 preflight와 명시 init/resume-init을 호출한다. 제품 설정 v1과 setup 초안 format은 변경하지 않는다.

대상은 Mac arm64의 사용자 범위 QBFT VALIDATOR·MTLS·RocksDB 개발 profile이다. [기존 제품/native 대응 규칙](./2026-09-18-product-engine-preflight.md)을 그대로 요구하며 native·PKI·peer/pin을 추측 생성하지 않는다. source-free provisioning 자료 준비는 BXDL 제품 후속 책임이다. NIGO의 canonical/key/chain/합의 판정을 대신 구현하거나 공급자의 요청 상태를 변경하지 않는다.

이번 구현은 BX-021/022/024/030의 등록·단발 초기화 연결 부분이다. launchd·run/start/status/stop, 전체 R2/R3·G1-M 완료, Linux G1-L/systemd, Docker G1-D는 포함하지 않는다.

## 2. CLI와 공개 API

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

`--instance`는 제품의 instanceId가 아닌 명시 제어 폴더 경로다. 안내와 후속 호출은 절대경로를 사용한다. package의 고정 위치 `engine/nigo-node.jar`, `runtime/bin/java`를 사용하므로 등록 후 JAR/Java 경로를 다시 받지 않는다. 신뢰 archive·key·engine lock은 package 밖의 자료다. 등록 시 개발 후보 및 선택적인 unsigned fixture 사용을 명시하고 보존한다. init/resume에는 각 작업 확인 flag가 필수다.

`engine::instance`는 `register(&RegisterOptions)`, `show(&Path)`, `preflight(&Path, Duration)`, `initialize(&Path, resume: bool, Duration)`를 제공한다. preflight는 기존 ProductReport를 반환하고 나머지는 Summary를 반환한다. setup/installer가 이 API를 자동 호출하지 않는다.

| 명령 | 정상 envelope / exit | 의미 |
| --- | --- | --- |
| instance register | SUCCEEDED / 0 | 등록 기록 게시, initialization NOT_STARTED |
| instance show | SUCCEEDED / 0 | 저장 상태 조회 성공. UNKNOWN 기록도 조회 가능 |
| preflight --instance | INCOMPLETE / 5 | 등록 입력·전체 설치본·결합 cold 확인, runtime 미검사 |
| init / resume-init | SUCCEEDED / 0 | 초기화 후 저장소 종료 및 여러 결과의 일치 확인 |
| init / resume-init 불명 | UNKNOWN / 6 | 실행 결과 또는 결과 기록 불명, 기존 자료 보존 |

인자 오류는 exit 2, 잠금 중·입력 변경·재개 부적격 등 확정된 사전 거부는 exit 3이다. 등록형 preflight의 사전 조건 실패도 exit 3이며 기존 `preflight --config`의 local FAIL/exit 4와 구분한다. cold JVM timeout은 ENGINE_TIMEOUT/UNKNOWN/exit 6이다. 결과 출력 실패는 exit 7이며 작업 미실행의 증거가 아니다.

Summary는 `instanceId`, `initialization=NOT_STARTED|INITIALIZED|UNKNOWN`, 선택 `operationBusy`, `developmentOnly=true`, `serviceRegistration=NOT_REGISTERED`, `runtimeReadiness=NOT_CHECKED`, 정해진 `reason`, 선택 `attemptId/genesisHash`를 포함한다. 경로·pin 목록·private credential hash·child 원문은 노출하지 않는다. `show.operationBusy`는 그 순간 advisory lock 관측이며 노드 health가 아니다. init의 불명 결과는 busy를 추측해 표시하지 않는다.

## 3. 등록과 입력 고정

등록은 persistent control/data 쓰기 전에 다음을 확인한다.

1. 제품/native/chain과 참조 credential·외부 key를 제한된 크기·안전한 경로로 읽고 SHA-256 pin을 만든다. private credential 내용의 hash는 private binding에만 저장하며 credential 원문은 저장하지 않는다.
2. 원본 archive를 외부 key 또는 명시 unsigned 정책으로 검증한다. archive/manifest hash, JAR/Java pin, source identity가 신뢰 engine lock과 일치해야 한다.
3. 설치 receipt뿐 아니라 설치된 전체 manifest의 path/size/hash/mode/inventory를 검증한다.
4. 제품/native 대응과 pinned engine-info·cold 결과를 확인한다. instanceId, backend, canonical chainFingerprint, nodeIdentity를 등록한다. 파일 raw hash와 canonical chainFingerprint를 혼동하지 않는다.
5. 데이터가 없거나 비어 있고 부모가 존재하는지 확인한다. 기존/외부 DB는 새 등록으로 채택하지 않는다. 입력 및 설치본 재확인 후 새 control root와 private binding·초기 journal을 게시한다. 기존 root는 거부한다. 등록 중 데이터는 생성하지 않는다.

원본 archive와 외부 공개 key도 이후 계속 필요하다. 설치 receipt는 재검증을 대체하는 신뢰 루트가 아니다. 등록 후 preflight·init/resume는 원본 archive, 고정 입력, 전체 설치본을 다시 확인하고 작업 뒤에도 전체 설치본·입력을 확인한다. 변경된 config/key/package를 자동 수용하거나 새 hash로 갱신하는 명령은 없다.

control/data는 입력·참조 자료·설치 package와 겹칠 수 없고, 신뢰 archive/key/lock은 package와 분리한다. symlink·Mac path alias를 고려한다. 이 경계와 same-UID 한계를 구분한다.

## 4. 제어 기록과 상태 전이

```text
<control>/
  .control.json          # root/lock/operations identity
  .operation.lock        # 생성 시 고정, 삭제·교체·명시 unlock하지 않음
  binding.json           # 경로·고정 hash·engine identity, 0600
  journal/               # append-only 상태 checkpoint
  operations/<attempt>/  # JAR/config/chain snapshot, report, 검증 결과
<data>/engine-instance.json  # 별도 NIGO journal
```

root/operations/journal은 0700이며 BXDL private 기록은 0600이다. cap-std의 열린 디렉터리 기준으로 작업하고 dev/inode·mode·regular file/link 조건을 재확인한다. `.control.json`은 inode 교체를 탐지하는 로컬 기록이며 같은 UID가 변경할 수 있는 파일이다. 암호학적 상태 로그·동일 UID 공격자에 대한 격리로 간주하지 않는다. 복사·이동·복원·journal 수동 조정의 지원 절차는 없다.

| 저장 phase | 허용 작업과 다음 상태 | 공개 initialization |
| --- | --- | --- |
| REGISTERED | fresh init 사전 조건 확인 → INIT_INTENT | NOT_STARTED |
| INIT_INTENT | 실행/결과 확인 중 또는 CLI 중단. 명시 resume의 추가 조건 검사 가능 | UNKNOWN |
| UNKNOWN | 기존 결과 보존. 명시 resume의 추가 조건 검사 가능 | UNKNOWN |
| INITIALIZED | show/preflight 가능. init/resume 반복 거부 | INITIALIZED |

fresh init은 없는 데이터 또는 정확히 0700의 빈 일반 디렉터리만 받는다. 부모는 존재해야 한다. journal의 intent와 terminal checkpoint 용량을 먼저 확보한다. 새 attempt workspace와 pinned JAR/config/chain을 준비하고 모든 입력을 재확인한 뒤 INIT_INTENT를 내구성 있게 저장한다. intent 저장 결과가 불명이면 엔진을 시작하지 않는다. intent 뒤에만 0700 data 준비와 NIGO init을 수행하며 기존 권한 수정·삭제는 하지 않는다.

resume-init은 BXDL phase가 INIT_INTENT/UNKNOWN이고 같은 backend/chainFingerprint/nodeIdentity/dataDirectory의 NIGO journal이 INITIALIZING이며 genesisHash가 비어 있어야 한다. `engine.lock`과 기존 `ledger/CURRENT`도 필요하다. NIGO가 실제 ledger를 열고 canonical 초기화 상태를 판단한다. missing ledger/genesis를 BXDL이 복구하거나 INITIALIZED를 재개하지 않는다. 매 재개는 같은 등록 아래의 새 attempt다. 외부 INITIALIZING DB를 새로 등록해 resume하는 흐름은 제공하지 않는다.

등록/실행 사전 실패는 새 intent를 기록하지 않을 수 있다. 반대로 intent 이후 오류는 실패 원인이 명확해 보여도 데이터 영향과 최종 게시 상태를 단정하지 않고 UNKNOWN으로 기록한다. 기존 시도와 데이터는 남긴다.

## 5. 프로세스와 작업 잠금

`.operation.lock`은 nofollow·0600·일반 파일·단일 link로 고정하고 nonblocking exclusive flock을 사용한다. 이미 잠겼다면 INSTANCE_BUSY다. child stdin에는 동일 open file description의 복제 File을 전달한다. Controller가 사라져도 Java가 fd 0을 유지하는 동안 작업 잠금이 살아 있다. Drop에서 LOCK_UN을 호출하지 않고 마지막 descriptor가 닫힐 때 해제되게 한다.

이 방식은 선택한 JRE와 pinned NIGO가 init/resume 중 stdin을 닫거나 재할당하지 않는다는 실행 조건을 가진다. source `303e163a`의 생산 초기화 경로 확인과 실제 Java 상속 시험의 범위를 실제 JAR 수용 시험과 분리해 기록한다. JRE/engine 변경 때 이 조건을 다시 검증해야 한다. advisory lock은 협조하지 않는 직접 실행·같은 UID 변경을 막는 sandbox가 아니다.

실행은 지정 Java·snapshot JAR·명시 `--config=`, 새 `--report=`, `--attempt-id=`만 사용하며 ambient JVM 옵션을 제거한다. bounded stdout/stderr와 private workspace를 사용하고 child 원문을 오류에 반사하지 않는다. cold adapter의 종료 정책과 쓰기 작업의 정책을 구분한다.

등록/cold timeout은 기본 30초·최대 120초다. init/resume timeout은 기본 120초·최대 600초이며 사전 cold는 각 호출 최대 120초다. 초기화 timeout 또는 출력 한도 초과 시 소유한 직접 child에 TERM을 보내고 최대 5초를 더 기다린다. SIGKILL하지 않으며, 살아 남은 child가 잠금을 보유할 수 있다. UNKNOWN을 반환해도 종료·저장소 close·lock 해제를 보증하지 않는다. 자동 재시작·데이터 cleanup은 없다.

## 6. 성공 증명과 불명 상태

초기화 성공은 다음 증거를 같은 실행에 결속해야 한다.

- 직접 실행한 child가 종료했고 exit 0이다.
- stdout이 정확한 INITIALIZED/attemptId/genesisHash 형식이다.
- 해당 attempt report가 같은 command·PID·연속 sequence의 CHECKING → STARTING → INITIALIZED/STORAGE_CLOSED 세 완결 행이며 build identity/backend/chainFingerprint/nodeIdentity/genesisHash가 일치한다. trailing partial/추가 행·unknown field는 성공으로 받지 않는다.
- `engine-instance.json`이 같은 dataDirectory/backend/chainFingerprint/nodeIdentity와 INITIALIZED/genesisHash를 담는다.
- 원본 입력·binding·control·report·엔진 journal·전체 설치본의 후속 검사가 통과한다.
- private 검증 결과와 BXDL INITIALIZED checkpoint가 저장된다.

report의 INITIALIZED 행만으로는 실제 process exit/stdout과 storage close 후 결과 전달까지 확인할 수 없다. CLI crash 후 NIGO가 INITIALIZED를 남겨도 BXDL이 성공을 자동 채택하지 않는다. `instance show`는 기록을 고치지 않는다. 부적격 resume·손상된 control·잃은 출력에 대한 수동 adopt/reconcile/restore API는 이번에 제공하지 않는다.

각 attempt의 JAR·설정 snapshot·report·검증 결과는 성공 여부와 무관하게 유지한다. 현재 JAR가 약 160 MB이므로 반복 시도는 그만큼 디스크를 사용한다. 자동 GC·보존 정책·지원용 redacted export는 후속이다. raw 설정/secret 참조가 있는 private 기록을 공개 보고서로 취급하지 않는다.

## 7. 검증 근거와 다음 단계

이 문서는 구현 계약이다. 실제 테스트·JAR 실행·중단 주입 결과는 [이번 검증 기록](../results/2026-09-18-instance-initialization.md)에 별도로 남긴다. [이전 cold 결과](../results/2026-09-18-clean-engine-preflight.md)는 당시의 DB 미실행·peer 없는 단일 validator fixture 근거로 보존하며 초기화 인수로 소급하지 않는다. 로컬 빌드/type-check는 원격 CI·Linux 실행·Mac 서비스 인수가 아니다.

이후 clean 후보의 runtime/종료 계약에 연결하는 수동 LaunchAgent start/status/stop을 [별도 설계](./2026-09-18-macos-launchagent.md)로 구현했다. 해당 구현과 단일 validator의 같은 데이터 재시작 근거는 [최신 상태](../docs/implementation-status.md)를 따른다. 전체 G1-M·4-validator 회귀, setup의 package 선택·등록 연결, native/PKI/거래 fixture 준비, 정식 Java 21 공급자·최소 macOS·patch/hash·NOTICE/SBOM 선정은 남아 있다. Linux/systemd와 Docker는 별도 후속 gate로 유지한다.
