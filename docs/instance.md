# 개발 인스턴스 등록과 초기화

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

## 저장 위치와 현재 경계

제어 폴더에는 `binding.json`, append-only `journal/`, 시도별 `operations/<attempt>/`, 작업 잠금과 파일 identity 기록이 있다. 제어 폴더·시도 폴더는 0700, private 기록은 0600이다. 엔진 report와 `engine-instance.json`은 NIGO가 작성하는 별도 자료다. CLI stdout/stderr에는 원문 설정·secret 경로·child 원문 출력을 내보내지 않는다.

각 시도는 실행 JAR와 설정/chain snapshot을 남기며 현재 후보 JAR만 약 160 MB다. 성공·실패 시도에 대한 자동 GC는 없다. 디스크 여유를 확보하고 작업 중에는 이 자료를 옮기거나 지우지 않는다. inode에 고정한 제어 기록은 복사본을 그대로 복원해 사용하는 형식이 아니며, 인스턴스 이동·복구 절차도 아직 제공하지 않는다.

CLI와 Java는 같은 Mac 사용자 UID로 실행한다. 파일 권한·hash·advisory 잠금은 실수와 일반 동시 실행을 차단하지만 같은 UID에 대한 강한 격리나 암호학적으로 보호된 상태 로그가 아니다. 잠금 상속은 선택한 Java/NIGO가 작업 중 stdin을 닫거나 바꾸지 않는 실행 조건에 의존한다.

이번 범위는 development 후보의 등록과 단발 초기화다. setup의 package 선택·자동 등록, 설정/키 변경과 재등록 migration, launchd 등록·start/status/stop, G1-M 4-validator 인수, 정식 JRE 선정은 후속이다. [설계](../design/2026-09-18-instance-initialization.md)와 [검증 기록](../results/2026-09-18-instance-initialization.md)은 구현 및 실제 실행 근거를 구분한다.
