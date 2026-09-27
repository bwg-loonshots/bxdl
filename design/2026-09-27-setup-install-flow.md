# setup 설치 흐름과 중단·재개

2026-09-27 · 기존 초안·installer·등록/초기화·Mac LaunchAgent 위에 추가한 A~C 구현.

## 범위와 사용자 계약

Mac arm64 사용자가 하나의 터미널 도우미에서 준비한 패키지·설정을 선택하고 설치·등록·초기화·시작을 각각 승인한다. 설치만 하거나 초기화 후 시작하지 않고 종료하는 것도 정상 경로다. 저장 성공, 초기화 완료, 실행 상태와 로컬 readiness를 구분하며 전역 합의는 `NOT_CHECKED`로 남긴다.

```sh
bxdl setup --install --workspace <new-session> [--from <product.json>]
bxdl setup --install --workspace <session> --resume
```

`--install`은 TTY 전용이다. `--workspace`가 필요하며 `--json`, `--output`, `--non-interactive`, `--resume`과 `--from`의 혼용을 거부한다. 기존 `setup`의 초안 저장·로컬 검사·export·비대화형 동작과 Draft v1 schema는 유지한다. 기존 초안 폴더를 설치 세션으로 자동 변환하지 않는다.

이번 구현은 Mac의 명시 native QBFT VALIDATOR/mTLS·RocksDB 구성과 새 data 경로를 다룬다. engine 자동 업데이트, 기존 DB 업그레이드, native 설정·membership/peer/pin 자동 생성, PKI 발급은 포함하지 않는다. 소비자 근거의 clean NIGO `303e163a` pin을 유지하며 최신 NIGO source나 로컬 build JAR를 새 공급물로 자동 채택하지 않는다.

## 계획에서 조정한 경로 범위

최초 계획의 Application Support 상위 폴더 자동0700 준비는 구현하지 않았다. **workspace와 package/control/data 모두 기존 부모 폴더를 요구한다.** 각 출력 자체는 새 경로여야 한다. 패키지·control·data의 기본 제안은 workspace와 같은 부모 아래의 `<세션이름>-package`, `<세션이름>-instance`, `<세션이름>-data`다.

이 제한은 설치 도우미에 적용한다. 기존 기본 초안 모드의 Mac 기본 작업 폴더와 부모 준비 동작을 바꾸지 않는다. 사용자 입력의 상대경로는 최초 실행 위치에 결속하고, 가져온 JSON 참조는 원본 JSON 위치에서 해석한다. 다른 cwd에서 resume해도 저장된 입력 기준을 바꾸지 않는다.

## 구성과 저장 경계

| 구성 | 책임 |
| --- | --- |
| `setup/workflow.rs` | 입력·계획 pin, 단계 의도/결과, 소유 결과 대조와 기존 API 연결 |
| `setup/install_wizard.rs` | 한국어 입력·계획 화면, apply/init/resume-init/start의 개별 확인, 취소/종료 |
| `setup/workflow_store.rs` | container identity, 세션 전체 배타 잠금, workflow checkpoint |
| `setup/Session` | 기존 제품 초안 입력·검증·checkpoint |
| `engine/setup_plan.rs` | 아직 내보내지 않은 제품 설정과 native/lock/참조 자료의 읽기 전용 결속, CLI byte 일치 |
| installer·instance 내부 확장 | 새 root 생성 시 소유 identity callback, 완료된 소유 결과의 읽기 전용 검증 |

```text
<session>/                  0700
  .workflow.json            container·하위 폴더·lock·marker inode 결속
  workflow.lock             도우미 객체 수명 동안 exclusive flock
  workflow/                 append-only 설치 진행 기록
  draft/                    기존 Draft v1 전용 저장소
  generated/                계획별 instance-<revision>.json
```

파일은 0600이며 워크플로와 초안은 각각 기존 Store의 크기·revision 한도와 CAS를 사용한다. lock은 Store의 엄격한 파일 목록 밖에 둔다. 워크플로 객체를 닫기 전까지 다른 프로세스의 재개를 차단하고, 잠금 파일을 다음 실행 프로그램에 암묵적으로 넘기지 않는다.

create는 새 root를 먼저 예약하고 두 초기 checkpoint를 저장·fsync한 뒤 identity marker를 마지막 공개한다. 불완전한 생성이나 missing marker/lock을 open이 고치거나 채택하지 않는다. 이미 완료된 Store의 정상 crash-temp 복구 규칙은 유지한다. 공개 snapshot의 손상·동시 변경을 오래된 revision으로 되돌리지 않는다.

경로 component의 symlink, 특별 파일, 잘못된 소유자·권한, 외부 hardlink, root/하위 폴더/lock/marker의 inode 교체를 거부한다. generated는 private regular file만 저장하는 평면 디렉터리다. 같은 UID의 악성 프로세스에 대한 암호학적 격리나 backup 이동·복원 계약을 주장하지 않는다. 세션 파일을 수동 수정하거나 다른 위치로 복사하여 복구하지 않는다.

## 입력과 계획

1. archive, `signed`와 별도 신뢰 공개키 또는 명시 `unsigned-development`, engine lock, native node.json, package/control 경로를 받는다.
2. 독립 신뢰 정책으로 archive를 검증하고 manifest의 `bin/bxdl` hash와 현재 CLI bytes를 대조한다. 불일치하면 설치 전에 중단한다. 자동 CLI 교체·재실행은 없다.
3. 제품 14개 필드를 받거나 `--from` 자료를 사용한다. 제품/local 검사와 native QBFT 설정·chain/credential 참조를 대조한다.
4. 계획 revision과 제품 bytes hash, package archive/manifest hash, 신뢰 key와 native/lock/chain/credential pin, 새 출력 경로를 고정한다.
5. workspace·package·control·data와 원본 참조 사이의 겹침 및 Mac 경로 별칭을 검사한다. 기존 data는 새 설치 대상으로 받지 않는다.

계획 화면에는 engine source/dirty, 생성할 주요 경로, native의 자동 GC ON/OFF와 chain의 설정된 발행자 주소 수를 표시한다. GC를 toggle하거나 발행자 권한을 부여하지 않으며 주소 수를 on-chain 권한·거래 가능성의 증명으로 취급하지 않는다. 설정이 제공되지 않으면 미제공으로 남긴다.

출력 생성 전 `edit`/`inputs`는 기존 계획을 무효화한다. 사용자는 다시 `plan`과 해당 작업 확인을 거쳐야 한다. 원본 bytes가 계획 이후 바뀌면 기존 확인으로 실행하지 않는다. 출력 생성 후에는 이 세션에서 계획·등록 binding을 편집하지 않는다.

## 실행과 명시 동의

| 선택 | 쓰기·실행 경계 |
| --- | --- |
| `plan` | private 계획 checkpoint만 저장. 고객 출력·DB·서비스는 생성하지 않음 |
| `apply` + `y`/`yes` | 제품 JSON 생성 → 파일 설치 → 전체 inventory 재검증 → cold → 등록 |
| `init` + 별도 확인 | durable 의도 후 기존 instance 초기화 API 호출. 새 data만 허용 |
| `resume-init` + 별도 확인 | 기존 API가 허용하는 동일 identity의 INITIALIZING/ledger 재개 조건 검사 |
| `start` + 별도 확인 | 기존 LaunchAgent gate/start API 호출. 터미널 이후에도 서비스가 남을 수 있음 |
| `check` | 고정 입력·소유 결과와 필요한 서비스 상태를 재조회. init/start를 반복하지 않음 |
| `finish` | 현 단계 저장 후 정상 종료. 미실행 단계 예약 없음 |
| `:cancel` / EOF | 마지막 완료 기록 보존 후 중단. 이미 수행한 작업을 rollback하지 않음 |

cold 정상의 `INCOMPLETE`는 runtime/DB/native 미검사를 유지한다. 등록 API가 검증하는 알려진 cold 결과로 다음 단계를 연결하며 임의 exit/UNKNOWN을 성공으로 바꾸지 않는다. 초기화 성공과 실제 노드 시작을 분리하고 로컬 READY를 quorum/거래 확정으로 표시하지 않는다.

## 소유 증거와 재개

단계 상태는 `PENDING`, `IN_PROGRESS`, `CONFIRMED`, `UNKNOWN`으로 저장한다. UI의 완료 판정은 checkpoint 값뿐 아니라 고정 입력과 실제 소유 결과를 다시 대조한다.

installer와 register는 create-new로 얻은 root의 device/inode를 callback으로 알려준다. 워크플로는 그 소유 증거를 fsync한 뒤 본 작업을 계속하도록 한다. callback 실패 시 작업을 계속하지 않는다. mkdir와 callback의 영속 저장 사이에서 중단되면 소유 증거가 없으므로 `UNKNOWN`이다. 나중에 발견한 폴더의 inode를 읽어 소유 증거를 새로 만들지 않는다.

| 중단 시점 | 재개 판정 |
| --- | --- |
| 입력·계획만 저장 | 마지막 완료 답·계획에서 계속 입력 |
| 제품 JSON 생성 직전/직후 | private generated 경로의 저장 의도와 exact bytes/hash를 확인. 부재·불일치는 보존하고 자동 overwrite하지 않음 |
| 설치/등록 완료 뒤 workflow 완료 기록 전 | 생성 시점 소유 identity와 전체 receipt/inventory/binding을 함께 검증한 경우만 완료 재구성 |
| partial 설치·등록 또는 소유 증거 없음 | `UNKNOWN`, 자동 재설치·삭제·외부 결과 채택 없음 |
| 초기화가 불명 | 기존 instance 판정 보존. 자동 init·새 genesis·성공 채택 없음 |
| start 후 controller 종료 | 같은 등록의 status부터 확인. start를 중복 실행하지 않음 |
| 원본 입력·설치본 변경 | 이전 계획으로 진행하지 않음. 등록 후 migration은 별도 기능 |

`finish` exit 0은 세션 저장 성공이며 모든 단계 완료가 아니다. 취소/EOF는 exit 5다. 화면에 표시된 설치·등록·초기화와 관측 상태를 별도로 확인한다. 세션의 생성 config, 원본 archive/key/lock/native와 참조 자료는 등록 이후에도 필요하므로 임시 자료처럼 지우지 않는다.

## 검증 범위와 후속

A는 경로/잠금/append-only 저장과 기존 초안 호환, B는 새 설정·설치·등록의 명시 연결과 소유 결과, C는 명시 초기화·시작과 재개 UX다. source 수준 회귀에는 동시 재개, marker/lock/경로 교체, symlink/hardlink/FIFO, 중단 생성·공개 snapshot 손상, 크기/revision 예산, 신뢰·CLI 불일치와 동의·EOF 경계를 포함한다.

최종 검사 결과와 실행 환경은 [별도 결과 기록](../results/2026-09-27-setup-install-flow.md)을 따른다. 이번 CLI의 새 개발 package/TTY로 정상 흐름·공백 경로·취소/재개·동일 DB 재시작을 확인했다. D의 signed 실제 package·전체 문답·강제 중단/실패 주입은 후속이며 이 설계 자체나 이전 init/LaunchAgent 시험을 새 setup의 실제 인수 근거로 사용하지 않는다. 최신 NIGO 교체 시에는 별도 exact JAR/contract/runtime pin과 소비자 회귀가 필요하다.

전체 Mac G1-M·4-validator 거래/재시작, 정식 JRE/NOTICE/SBOM·최소 OS 선정, Linux/systemd·Docker, Application Support 부모 자동 준비, native/PKI 도우미, 기존 DB migration·업데이트·offline 유지보수, partial 자동 복구·삭제와 보존 용량 정책은 후속이다. 이번 변경에는 NIGO 소스·요구 원장 수정이 없다.
