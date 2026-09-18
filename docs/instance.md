# 개발 인스턴스 등록·초기화와 Mac 서비스

macOS arm64에서 설치한 개발 package와 제품/native 설정을 한 인스턴스로 등록한다. 이후에는 `--instance` 경로로 검사·초기화를 호출하므로 JAR·Java·설정 경로를 매번 입력하지 않는다. **초기화는 데이터 저장소를 만들고 닫는 한 번의 작업**이며 노드나 OS 서비스를 시작하지 않는다.

## 등록 전에 준비할 자료

[setup](./setup.md)으로 만든 제품 `instance.json`, 운영자가 준비한 NIGO native `node.json`·공개 chain·validator NGVK/TLS 자료, [설치](./install.md)를 마친 package, 원본 archive와 외부 신뢰 공개 key, 선택한 JAR/JRE에 맞는 외부 engine lock이 필요하다. 제품/native는 [QBFT VALIDATOR·MTLS 대응 규칙](../design/2026-09-18-product-engine-preflight.md)을 만족해야 한다. 키·PKI·peer/pin·native 설정을 자동 생성하지 않는다.

등록 기록, 데이터, 설치 package, 설정·키 자료는 서로 겹치지 않는 위치를 사용한다. 원본 archive·신뢰 공개 key·engine lock은 설치 package 밖에 둔다. 경로의 symlink를 허용하지 않으며 Mac의 대소문자·Unicode 별칭 충돌도 검사한다. 아래 예시의 절대경로는 준비한 자료의 실제 경로로 바꾼다. 새 인스턴스 제어 폴더와 데이터 폴더의 부모는 미리 존재해야 한다.

```bash
bxdl instance register \
  --instance "/absolute/bxdl/instances/validator-a" \
  --package "/absolute/bxdl/releases/candidate-01" \
  --archive "/absolute/bxdl/downloads/candidate-01.tar.gz" \
  --public-key "/absolute/bxdl/trust/release-public.pem" \
  --config "/absolute/bxdl/config/instance.json" \
  --engine-config "/absolute/bxdl/config/node.json" \
  --lock "/absolute/bxdl/trust/engine.lock.json" \
  --allow-development --json
```

서명 없는 개발 fixture에만 `--public-key` 대신 `--allow-unsigned-development`를 쓴다. 둘을 동시에 지정할 수 없다. `--allow-development`는 별도로 필수이며 등록된 개발용 선택을 후속 명령에서 사용한다. init 때 이 옵션들을 반복할 필요는 없다. 등록·등록 후 preflight의 `--timeout-seconds`는 기본 30초, 범위 1~120초이며 각 cold JVM 호출에 적용한다.

등록은 원본 archive의 서명/전체 inventory, 설치 receipt와 **설치된 전체 manifest 파일**, engine lock·engine-info identity, 제품/native 일치와 cold 결과를 검사한다. 등록 시에도 data는 없거나 빈 디렉터리여야 하며 부모는 존재해야 한다. 기존/외부 DB를 새 등록에 채택하지 않는다. 검사 뒤 새 0700 제어 폴더에 등록 정보와 journal을 저장한다. 기존 폴더에는 등록하지 않는다. 데이터 디렉터리는 만들거나 초기화하지 않는다.

등록 성공은 exit 0, `INSTANCE_REGISTERED`, `initialization=NOT_STARTED`다. 그 안에 사용한 cold 결과는 여전히 INCOMPLETE이며 service/readiness를 완료로 올리지 않는다. 원본 archive·공개 key·제품/native·chain·참조 키 자료는 이후 preflight/init에도 같은 위치와 내용으로 필요하다. 등록 정보에 경로와 내용 hash를 고정하고, private credential 내용의 hash도 포함한다. 원문 credential을 등록 정보에 저장하거나 hash/secret 경로를 명령 결과로 표시하지 않는다.

## 등록 상태와 사전 검사

```bash
bxdl instance show --instance "/absolute/bxdl/instances/validator-a" --json
bxdl preflight --instance "/absolute/bxdl/instances/validator-a" --json
```

`show`는 저장된 상태를 읽고 `operationBusy`로 그 순간의 advisory 작업 잠금을 관측한다. exit 0은 조회 성공이다. `operationBusy=false`는 노드 종료·정상·포트 상태의 증명이 아니다. `initialization=UNKNOWN`인 기록도 조회 자체는 성공한다.

등록 후 preflight는 원본 archive·입력 hash·설치된 전체 파일을 다시 검증하고 기존 제품/native 결합 cold를 수행한다. 정상 결과는 exit 5/INCOMPLETE다. `--instance` 방식과 `--config/--engine-config/--jar/--java/--lock` 방식은 섞지 않는다. 이 명령은 초기화 상태를 고치거나 UNKNOWN을 성공으로 바꾸지 않는다.

## 새 데이터의 명시 초기화

```bash
bxdl init --instance "/absolute/bxdl/instances/validator-a" \
  --confirm-initialize --json
```

등록 직후에만 실행할 수 있다. 데이터는 없거나 정확히 0700인 빈 일반 디렉터리여야 하며 부모는 존재해야 한다. 기존 데이터, symlink, 부적합 권한을 자동 삭제·수정하지 않는다. 작업 의도를 먼저 journal에 내구성 있게 저장한 뒤 없는 데이터 폴더를 0700으로 준비하고, NIGO `init`을 호출한다.

init/resume-init의 timeout은 기본 120초, 범위 1~600초다. 사전 cold 호출은 각각 최대 120초로 제한되며, 이 옵션은 전체 명령의 총 소요 시간 제한이 아니다. 초기화 JVM이 시간 한도를 넘으면 TERM을 보내고 최대 5초 더 종료를 기다린다. **SIGKILL은 보내지 않는다.** 프로세스가 계속 살아 있으면 작업 잠금도 남을 수 있다.

BXDL은 직접 실행한 child의 종료와 exit 0, 제한된 stdout, 같은 attempt·PID의 완결 report, 데이터의 `engine-instance.json` 및 genesis/identity 일치를 확인한다. 작업 후 입력과 설치된 전체 파일도 다시 확인한다. 모두 일치하고 결과 기록까지 완료해야 `INSTANCE_INITIALIZED`/exit 0이다. `serviceRegistration=NOT_REGISTERED`, `runtimeReadiness=NOT_CHECKED`는 그대로다.

## 중단과 UNKNOWN

초기화 의도를 기록한 뒤 timeout, 비정상 종료, 출력·report 불일치, 입력 변경 또는 결과 기록 불확실성이 생기면 exit 6/UNKNOWN으로 다룬다. 프로세스가 실행되지 않았더라도 의도 기록의 게시 여부가 불명일 수 있다. 데이터와 시도별 기록을 보존하며 자동 재실행·삭제·복구하지 않는다. 출력 자체의 실패(exit 7)나 CLI 강제 종료도 작업 실패/미실행을 증명하지 않는다.

1. 같은 경로의 `instance show`로 저장 상태와 잠금을 확인한다. 잠겨 있으면 다른 초기화 명령은 `INSTANCE_BUSY`로 거부한다. 잠금 파일을 삭제해 우회하지 않는다.
2. 잠금이 풀려도 UNKNOWN을 완료로 해석하지 않는다. CLI가 사라진 뒤 엔진 journal/report에 INITIALIZED가 남아 있어도 종료·stdout 증거를 잃었다면 BXDL은 이를 자동 채택하지 않는다.
3. 같은 identity의 NIGO journal이 **INITIALIZING**이고 기존 `engine.lock`·`ledger/CURRENT`가 있는 경우에만 아래 명시 재개를 시도할 수 있다. 재개 명령도 모든 신뢰·입력 조건을 다시 검사한다.

```bash
bxdl resume-init --instance "/absolute/bxdl/instances/validator-a" \
  --confirm-resume --json
```

`resume-init`은 해당 등록의 미완료 시도만 대상으로 하며 외부 INITIALIZING 데이터의 새 등록·인수를 제공하지 않는다. 재초기화 버튼이 아니다. 새 데이터·ledger 누락·INITIALIZED·identity 불일치에는 적용하지 않는다. 재개할 수 없는 UNKNOWN의 수동 조정/채택·rollback·복구 명령은 아직 없다. 기록과 데이터를 보존한 채 별도 검토가 필요하다. 이미 성공한 인스턴스의 init/resume-init 반복도 거부한다.

## 초기화 후 시작·조회·종료

현재 사용자가 GUI 세션에 로그인한 macOS arm64에서 명시적으로 LaunchAgent를 시작한다. **start에 사용하는 CLI는 등록된 설치 package의 `bin/bxdl`과 같은 바이트여야 한다.** 아래 경로는 서비스 명령을 포함한 package로 새로 설치·등록·초기화한 경로로 바꾼다. 기존 초기화용 package에 새 CLI만 덮어쓰거나 기존 DB를 새 등록에 채택하는 업데이트는 지원하지 않는다.

```bash
BXDL_PACKAGE="/absolute/bxdl/releases/candidate-02"
BXDL_INSTANCE="/absolute/bxdl/instances/validator-a"
"$BXDL_PACKAGE/bin/bxdl" start --instance "$BXDL_INSTANCE" --json
"$BXDL_PACKAGE/bin/bxdl" status --instance "$BXDL_INSTANCE" --json
"$BXDL_PACKAGE/bin/bxdl" stop --instance "$BXDL_INSTANCE" --json
# stop이 정상 종료를 검증하고 등록 정리를 마친 뒤에만 같은 데이터로 다시 시작한다.
"$BXDL_PACKAGE/bin/bxdl" start --instance "$BXDL_INSTANCE" --json
```

각 start는 별도 attempt·worker/JAR snapshot·plist·report를 만든다. 내부 worker는 control 잠금·자신의 snapshot·정확한 job/PID를 확인해 한 번 실행 허가를 먼저 기록한다. 이후 등록 입력·원본 archive·설치 전체 파일·cold 결과·INITIALIZED/genesis를 검사하고 Java로 전환한다. 검증 실패도 소비한 시도로 보존한다. worker와 Java의 PID가 같고 Java가 control 작업 잠금을 이어받는다. 기존 데이터 존재만으로 init 성공을 추측하지 않는다. 원본 archive/key와 등록 때 고정한 참조 자료는 계속 필요하다.

plist는 control의 시도 폴더에 두고 수동 bootstrap한다. `~/Library/LaunchAgents`에는 설치하지 않는다. 로그인 자동 시작·자동 재시작은 없으며 내부 `service-run`이나 `launchctl kickstart`로 이미 소비한 attempt를 다시 실행할 수 없다. 이 profile은 로그인 세션이 없는 장기 서버용 LaunchDaemon이 아니다.

start timeout은 기본 120초(1~600초)이며 사전 검증·bootstrap 뒤 시작 관측에 적용한다. 만료는 실행 취소가 아니며 worker/Java가 계속 진행할 수 있다. status는 기본 10초(1~120초), stop은 기본 60초(1~600초)의 공통 관측 예산을 사용한다. stop은 의도 기록 후 정확한 job을 확인해 TERM을 요청하며, timeout에 SIGKILL이나 live job bootout을 하지 않는다.

status는 해당 attempt의 launchd·report·로컬 bootstrap/health를 대조한다. 로컬 READY/exit 0은 전역 quorum·거래 확정·4-validator 정상의 증명이 아니다. STOPPED 조회도 exit 0일 수 있으므로 `engineState/runtimeReadiness/reason`을 확인한다. INCOMPLETE는 exit 5, 관측된 FAILED는 4, 관측 불명은 6이며 입력·사전 조건 거부는 3이다. `instance show`는 초기화 기록과 advisory busy만 보여주므로 노드 상태에는 `status`를 사용한다.

다음 start는 이전 시도가 `STOPPED_VERIFIED`여야 한다. stop이 **동일 시도의 정상 STOPPED report + launchd의 살아 있는 프로세스 부재 + control 잠금 획득**을 함께 확인해 이를 먼저 게시한다. 이후 프로세스 부재를 재확인하고 job 등록을 정리한다. status만으로는 이 기록을 게시하지 않는다. 검증 뒤 등록 정리만 실패했다면 종료 근거는 보존하며, 명시 stop 재호출로 정리를 다시 확인한다. 다음 start도 이전 job의 비활성과 등록 정리를 확인한 뒤 새 시도를 준비한다.

gate 거부·bootstrap 불명·부적합 report·정상 종료 미확인은 시도 자료와 함께 보존한다. 새 start, init/resume-init, 잠금 파일 삭제, plist 수정으로 자동 복구하지 않는다. gate 전에 실패해 엔진이 시작하지 않은 경우도 별도 수동 조정 기능은 아직 없다. 로그아웃·OS 종료 시 launchd가 자체 종료 또는 강제 종료할 수 있으며 `ExitTimeOut=60`은 CLI timeout과 별개다. 로그인/로그아웃·sleep/wake·전체 G1-M 인수는 아직 남아 있다.

## 저장 위치와 현재 경계

Mac 사용자 자료는 `$HOME/Library/Application Support/BXDL` 아래에서 package·control·data·config를 서로 분리해 두는 것을 권장한다. 등록 경로는 계속 명시하며 기본 설치/등록 위치를 자동 선택하지 않는다. 경로 접근 권한과 선택한 macOS에서의 LaunchAgent 실행 가능성은 별도 인수 조건이다.

제어 폴더에는 `binding.json`, append-only `journal/`, 시도별 `operations/<attempt>/`, 작업 잠금과 파일 identity 기록이 있다. 서비스는 별도의 runtime journal·시도별 plist/worker/report·private stdout/stderr를 보존한다. 제어 폴더·시도 폴더는 0700, private 기록은 0600이다. 엔진 report와 `engine-instance.json`은 NIGO가 작성하는 별도 자료다. CLI stdout/stderr에는 원문 설정·secret 경로·child 원문 출력을 내보내지 않는다. private 로그를 그대로 공개하거나 지원 자료로 제출하지 않는다.

각 시도는 실행 JAR와 설정/chain snapshot을 남기며 서비스 시도는 CLI snapshot과 private 로그도 남긴다. 현재 후보 JAR만 약 160 MB다. 성공·실패 시도와 로그의 자동 GC·용량 제한 정책은 후속이며 `logs/diagnose` 명령도 아직 없다. 디스크 여유를 확보하고 작업 중에는 이 자료를 옮기거나 지우지 않는다. inode에 고정한 제어 기록은 복사본을 그대로 복원해 사용하는 형식이 아니며, 인스턴스 이동·복구 절차도 아직 제공하지 않는다.

CLI와 Java는 같은 Mac 사용자 UID로 실행한다. 파일 권한·hash·advisory 잠금은 실수와 일반 동시 실행을 차단하지만 같은 UID에 대한 강한 격리나 암호학적으로 보호된 상태 로그가 아니다. 잠금 상속은 선택한 Java/NIGO가 작업 중 stdin을 닫거나 바꾸지 않는 실행 조건에 의존한다.

현재 범위는 development 후보의 등록·단발 초기화와 수동 LaunchAgent start/status/stop이다. 실제 새 package에서 단일 validator의 시작·정상 stop·같은 DB 재시작을 확인했으며 G1-M 4-validator·로그아웃/sleep 검증은 아직 완료하지 않았다. setup의 package 선택·자동 등록, 설정/키 변경과 재등록 migration, 로그/진단·제거, 정식 JRE 선정은 후속이다. [초기화 설계](../design/2026-09-18-instance-initialization.md)와 [그 검증 기록](../results/2026-09-18-instance-initialization.md), [LaunchAgent 설계](../design/2026-09-18-macos-launchagent.md)와 [서비스 검증 기록](../results/2026-09-18-macos-launchagent.md)은 각각의 범위를 구분한다.
