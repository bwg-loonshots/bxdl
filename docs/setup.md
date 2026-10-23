# 설정 초안과 설치 도우미

`bxdl setup`은 노드에 사용할 **제품 설정 초안**을 준비한다. 입력을 하나씩 저장하고 로컬 파일 상태를 확인한 뒤 새 JSON 설정으로 내보낼 수 있다. 현재는 validator·RocksDB 설정을 지원한다.

기본 `setup`은 초안 저장·내보내기까지만 수행한다. `setup --install`은 별도 설치 세션에서 패키지·등록·초기화·시작을 각각 명시적으로 선택한다. 어느 모드든 저장 성공을 모든 단계의 완료로 해석하지 않는다.

## 네트워크 설정 준비

제품 설정에 없는 validator·peer/pin·sync 입력은 별도 `setup --network` 도우미로 준비할 수 있다. 승인 chain과 전체 validator 공개 목록, 기존 키 자료를 받아 RC.1~RC.3 공통 계약의 native `node.json`을 생성한다. [네트워크 준비 가이드](./network.md)의 공개 인증서 pin 계산·문답·재개·설치 연결을 따른다. 기본 제품 초안 및 설치 세션 형식은 유지한다.

## 설치 모드: 패키지부터 시작까지

macOS arm64 터미널에서 실행한다. [지원하는 기존 RC.3](./management-cli.md)는 독립 관리 CLI로 설치한 뒤 고정된 패키지 실행기에 init/start/status를 위임한다. 도우미는 패키지 서명·내용과 정확한 실행기 profile을 검사한다. 다른 package는 설치할 `bin/bxdl`과 동일한 bytes의 CLI가 필요하며, 같은 버전 문자열만으로 허용하지 않는다. 패키지 내부 CLI를 덮어쓰지 않는다.

```bash
bxdl setup --install --workspace /absolute/existing-parent/new-session
# 기존 제품 설정으로 시작할 때
bxdl setup --install --workspace /absolute/existing-parent/import-session \
  --from ./instance.source.json
# 같은 설치 세션을 명시적으로 재개
bxdl setup --install --workspace /absolute/existing-parent/new-session --resume
```

`--workspace`가 필수이며 **부모 폴더는 이미 있어야 한다.** 기존 초안 폴더를 설치 세션으로 변환하지 않는다. `--resume`과 `--from`을 함께 사용하거나 설치 모드에 `--json`, `--output`, `--non-interactive`를 섞을 수 없다. 자동화는 기존 개별 install/register/init/start 명령을 사용한다.

설치·인스턴스·데이터 경로의 기본 제안은 작업 폴더와 같은 부모 아래의 `<세션이름>-package`, `<세션이름>-instance`, `<세션이름>-data`다. 변경한 경로도 기존 부모를 요구한다. `Library/Application Support/BXDL` 상위 폴더를 자동으로 만드는 설치 기능은 이번 범위에 포함하지 않았다. 기본 초안 모드의 작업 폴더 규칙과 구분한다.

| 순서 | 입력·선택 | 실제 동작 |
| --- | --- | --- |
| 설치 자료 | archive, `signed`와 외부 신뢰 공개키 또는 명시 `unsigned-development`, engine lock, native node.json, 새 설치·인스턴스 경로 | 패키지 신뢰·대상 플랫폼·지원 실행기 profile 또는 CLI 일치 확인 |
| 노드 설정 | 아래 14개 필드 또는 `--from`으로 가져온 제품 JSON | 제품 설정과 준비한 native QBFT VALIDATOR/mTLS·참조 파일 대조. 키나 peer 목록을 생성하지 않음 |
| 계획 | `plan` | 입력 hash와 새 출력 경로를 고정. 엔진 source, 자동 GC ON/OFF, 발행자 허용목록의 설정된 주소 수를 표시 |
| 파일 설치·등록 | `apply`, 이어서 `y` 또는 `yes` | 새 제품 설정 출력, 패키지 설치·전체 inventory 확인, NIGO cold 검사, 인스턴스 등록 |
| 초기화 | `init`, 이어서 별도 `y` 또는 `yes` | 등록한 새 데이터 경로에 명시 초기화. engine journal·report·종료 결과 확인 |
| 시작 | `start`, 이어서 별도 `y` 또는 `yes` | Mac 사용자 LaunchAgent 시작. 터미널 종료 뒤에도 실행될 수 있음 |
| 준비 상태 대기 | 시작 시도가 관측된 뒤 `wait` | 화면에 표시된 시도의 준비 상태를 최대120초 읽기 관측. init/start 재실행 없음 |
| 종료 | `finish` 또는 Enter | 현재 단계까지만 저장하고 종료. 미선택 초기화·시작을 예약하지 않음 |

자동 GC와 자산 등록 정책은 고정한 native/chain 입력의 안내다. 도우미가 GC를 켜거나 등록·발행 권한을 부여하지 않는다.
현재 수신·재인수 후보는 [NIGO RC.3](../contracts/nigo/engine-rc3-2026-10-22/README.md)이며, 선택한 lock/package만 사용한다.
기존 clean `303e163a` 및 RC.1/RC.2 입력과 설치 기록은 유지하고 최신 엔진으로 자동 교체하지 않는다.

RC.1~RC.3 공개 chain에는 `nigo.protocol.native-assets.registration-mode`를 정확히 `OPEN` 또는 `ALLOWLISTED`로 명시한다.
설치 계획은 모드·목록의 의미와 genesis 고정을 보여준다.

| 입력 | 계획에서 확인할 의미 |
| --- | --- |
| `OPEN`, 허용목록 0개 | 공개 등록 자격. 실제 등록 거래의 검증·성공은 별도 |
| `ALLOWLISTED`, 허용목록 0개 | 신규 자산 등록 불가 |
| `ALLOWLISTED`, 유효한 목록 | 목록에 있는 주소에 등록 자격 |
| 미지정·잘못된 값·중복·OPEN의 비어 있지 않은 목록 | 모드 미지정 또는 형식/정책 불일치 안내. RC.1~RC.3 호환으로 추론하지 않음 |
| 계획 이후 입력 변경·읽기 불가 | 확인 불가. 실행 전 기존 pin 검증을 계속 적용 |

모드·목록은 초기화 후 설정 편집으로 바꿀 수 없다. 등록 자격은 개별 자산 mint/burn 권한이나 RPC/P2P 접속 권한과 별개다.
기존 후보에 모드가 없다는 이유로 새 값을 자동 추가하지 않으며, RC.1~RC.3의 잘못된 입력은 공급 엔진 cold 검사에서 거절한다.
새 항목은 저장된 계획/저널 형식을 변경하지 않고 다시 확인한 입력에서 표시하므로 기존 계획의 재개 호환성을 유지한다.

`check`는 저장된 입력과 소유한 결과를 재검증하고, 시작 시도가 있다면 서비스 상태를 관측한다. `edit`는 노드 설정, `inputs`는 설치 자료·목적지에서 수정할 항목 번호를 선택한다. 선택한 한 항목을 저장하면 검토로 돌아가며 다른 답은 보존한다. 출력 생성 전에 답을 실제로 변경하면 기존 계획이 무효화되므로 다시 `plan`과 명시 확인이 필요하다. 도움말·뒤로가기·취소 또는 Enter로 기존 값을 유지한 경우에는 계획을 무효화하지 않는다. 출력 생성 이후에는 이 도우미에서 설정·등록 binding을 바꾸지 않는다.

시작 관측 기한이 끝나도 실행은 계속될 수 있다. 화면의 서비스·엔진·로컬 준비 상태와 시작 시도, 마지막 관측 사유를 확인한다. `wait`는 관측된 같은 시도를 기다리고 `check`는 다시 조회한다. 시도 변경·조회 오류를 이전 READY로 덮지 않으며, 새 start를 자동 수행하지 않는다. 완료되지 않은 시작의 기존 진행 기록과 계획 형식은 유지한다.

정상 cold 결과인 `INCOMPLETE`는 DB·native·runtime 미검사를 유지한다. 도우미는 등록 API의 알려진 cold 계약을 통해 진행하며 모든 실패나 INCOMPLETE를 일괄 성공으로 바꾸지 않는다. 설치 완료·등록 완료·초기화 완료·실행 상태·로컬 readiness는 별개이며 네트워크 합의는 계속 `NOT_CHECKED`다.

### 저장 후 중단과 재개

설치 모드의 `:cancel`이나 EOF는 마지막 완료 checkpoint를 보존한다. 이미 설치·초기화·시작한 작업을 되돌리지 않는다. Ctrl-C나 controller 종료 직후에도 서비스가 실행 중일 수 있으므로 같은 세션의 `--resume` 또는 `status --instance <등록폴더>`로 확인한다.

```text
<설치 세션>/             0700
  .workflow.json        생성한 폴더·잠금의 소유 identity 기록
  workflow.lock         세션 전체의 배타 잠금
  workflow/             설치 진행 checkpoint
  draft/                기존 Draft v1 checkpoint
  generated/            계획별 새 제품 JSON
```

파일은 0600이다. workflow와 draft는 각자 append-only 기록을 사용하며 동시 도우미 실행을 거부한다. 생성 중단으로 소유 표식·초기 기록이 불완전하면 자동 재개하거나 고치지 않는다. 기존 설치 중단 폴더도 덮어쓰거나 지우지 않는다.

재개는 생성 당시 기록한 root identity와 전체 완료 결과를 함께 확인할 수 있는 설치·등록만 완료로 재구성한다. 같은 내용의 외부 폴더를 발견했다고 채택하지 않는다. 소유 증거가 없거나 결과가 불명확하면 `UNKNOWN`을 보존한다. 초기화 불명 상태에는 자동 init을 하지 않으며, 표시된 `resume-init`을 별도로 확인해도 기존 엔진의 재개 조건을 통과해야 한다.

`finish`의 exit 0은 **현재 설치 세션 저장 성공**이다. 설치만 하거나 초기화 전 종료해도 정상이며 화면의 단계별 상태를 확인한다. 취소/EOF는 exit 5, 잘못된 옵션·비터미널 입력은 exit 2다. 단계 실패는 도우미에서 사유를 표시하고 보존하며, 파일 저장·세션 오류와 안내 출력 실패는 별도 오류로 반환할 수 있다.

새 등록(V2)은 원본 archive·신뢰 공개키·engine lock을 인스턴스의 `trust/`에 보관한다. 등록 완료 후 서비스와 설치 세션 재개는 이 보관본을 검증하므로 원본 세 파일의 위치에 의존하지 않는다. 등록 완료 전 원본과 완료 후 `trust/`는 보존해야 하며 archive 한 벌만큼 추가 공간이 필요하다. 완료된 설치 세션의 `generated/` 제품 JSON은 등록 binding이 참조하는 운용 입력이다. 설치가 끝나도 세션을 임시 폴더로 보고 삭제·이동하지 않는다. 이 제품 JSON과 설치본·control·data·native/chain·credential 참조는 계속 필요하다. 기존 V1 등록은 원본 세 파일도 계속 보존하며 자동 변환하지 않는다. 정상 종료는 `bxdl stop --instance <등록폴더>`로 확인하며 후속 관측은 `status`, `logs`, `diagnose`를 사용한다.

2026-10-21 인수에서는 `Documents` 아래 원자료를 참조한 설치의 LaunchAgent가 시작 전 파일 열기에서 대기했다. 경로 접근과 macOS 보호 폴더 권한의 관련성은 원인 미확정이며, 현재 CLI에는 서비스 관점의 보호 경로 사전 진단이 없다. 현재 구현은 위 V2 공급 자료 보관과 시작 단계 진단을 제공한다. `status`·setup·`diagnose`의 `startupStage`는 마지막 진입 검사 단계이며, 완료나 권한 문제의 원인 확정을 뜻하지 않는다. 설치본·control·운용 설정 자체의 보호 경로 접근은 별도 확인해야 한다. 시험에 사용한 임시 경로는 장기 운영 경로 권고가 아니다.

[RC.2 설치 UX 재검증](../results/2026-10-21-macos-rc2-setup.md)에서 전체 14문답·수정·취소/EOF·재개, 대기 중인 TTY 접속 종료, 명시 설치·초기화·시작·정상 종료·동일 DB 재시작의 20개 사례를 확인했다. 해당 실행의 자동 집계는 마지막 포트 확인 때문에 `INCOMPLETE`이며, 후속 독립 관측에서 서비스·프로세스·listener 정리를 확인했다. 앞선 원자료 경로 문제는 별도로 남아 있다. apply/init 진행 중 강제 중단·실패 주입, 로그아웃·sleep/wake와 전체 G1-M은 후속이다. [구현 설계](../design/2026-09-27-setup-install-flow.md), [최초 검증 기록](../results/2026-09-27-setup-install-flow.md)도 보존한다.

[RC.3 전체 설치 UX 재검증](../results/2026-10-22-macos-rc3-full-setup.md)에서는 고정한 RC.3 archive로 전체 14문답·취소/EOF·대기 중 HUP·재개·원본 공급 경로 부재 운용의 21사례와 독립 검사 8항목을 통과했다. 이는 당시 CLI/package의 결과이며 UX-01 변경 CLI의 패키지 인수를 대신하지 않는다. RC.2의 원본 `INCOMPLETE`와 경로 원인 미확정 기록은 보존한다.

## 기본 초안 모드

아래는 `--install`을 지정하지 않은 기존 초안 모드다. 패키지 선택·설치, 키 생성, DB 초기화, NIGO 검사·기동, launchd 등록을 수행하지 않는다.

## 시작과 재개

Mac 터미널에서 실행한다.

```bash
bxdl setup
```

기본 작업 폴더는 `$HOME/Library/Application Support/BXDL/setup`이다. 처음에는 새 폴더를 만들며, 이미 있으면 덮어쓰지 않는다. 같은 초안을 이어가려면 `--resume`을 붙인다.

```bash
bxdl setup --resume
```

여러 초안을 준비하려면 서로 다른 작업 폴더를 지정한다. 작업 폴더는 DB·체인 자료·키 파일과 분리한다.

```bash
bxdl setup --workspace "$HOME/Library/Application Support/BXDL/setup-validator-two"
bxdl setup --workspace "$HOME/Library/Application Support/BXDL/setup-validator-two" --resume
```

대화형 입력에는 터미널이 필요하다. 파이프 입력이나 자동화에서는 아래 비대화형 방식을 사용한다. Mac 이외의 개발 환경에서는 `--workspace`가 필수이며, 실행 가능 여부를 Linux 서비스 설치 지원으로 해석하지 않는다.

## 입력할 자료

14개 항목을 차례로 입력한다. 비밀번호·private key 원문을 입력하지 말고 준비한 파일의 경로를 사용한다.

| 항목 | 입력 내용 |
| --- | --- |
| 1. 인스턴스 이름 | 소문자로 시작하는 1~32자의 소문자·숫자·하이픈. 기본값 `validator-one` |
| 2. 공개 노드 ID | 운영자가 준비한 공개 식별자. 엔진 결합 검사에는 `0x`로 시작하는 32-byte hex가 필요하며 validator ID와 다름 |
| 3. 데이터 저장 위치 | 사용할 DB 경로. Mac 기본 제안은 `Library/Application Support/BXDL/instances/<이름>/data`; 이 디렉터리나 DB를 생성하지 않음 |
| 4~5. 노드 HTTP IP·포트 | `127.0.0.1` 또는 `::1`, 1~65535. 기본 `127.0.0.1:18080` |
| 6~7. P2P IP·포트 | 명시적인 unicast IP와 콘솔과 다른 포트. IP 기본값은 없으며 포트 기본값은 `19090`; Mac 로컬 시험에 `127.0.0.1` 사용 가능 |
| 8. 공통 체인 자료 | 기존 파일 경로. 새 네트워크·validator membership을 생성하지 않음 |
| 9~10. Validator 자료 | 기존 NIGO NGVK keystore와 비밀번호 파일 경로. TLS용 PKCS12와 다른 형식 |
| 11~12. TLS 키 자료 | 기존 PKCS12/JKS keystore와 비밀번호 파일 경로; 실제 type은 native 설정에 맞춤 |
| 13~14. TLS 신뢰 자료 | 기존 PKCS12/JKS truststore와 비밀번호 파일 경로 |

여기서 노드 HTTP IP·포트는 NIGO 엔진의 로컬 HTTP 주소다. 후속 `bxdl console`은 터미널 문답 진입점이며 별도 웹서버를 뜻하지 않는다.

Enter는 저장된 답 또는 표시한 기본값을 유지한다. 필수 입력이 없거나 형식이 잘못되면 해당 항목을 다시 묻는다. 자료 파일이 아직 없으면 경로를 입력해 초안을 준비할 수 있지만 로컬 검사 결과에는 누락이 남는다. 파일의 존재와 권한을 확인할 뿐 내용·인증서·키의 유효성을 검증하지 않는다.

| 입력 | 동작 |
| --- | --- |
| `:back` | 순차 질문에서는 앞 항목, 그 외에는 이전 입력 또는 검토 단계로 돌아감. 저장한 답은 보존 |
| `:help` | 현재 질문·선택의 도움말을 보고 같은 입력으로 돌아감 |
| `:cancel` | 마지막으로 저장된 답을 보존하고 종료 |
| EOF(Ctrl-D) | 마지막 저장 상태를 보존하고 종료 |

이 공통 입력은 제품 초안·설치 자료·네트워크 질문과 검토·출력·확인에서도 사용할 수 있다. `:help`, `:back`, `:cancel` 자체를 파일명으로 지정하려면 `./:help`처럼 명시 경로로 입력한다. 명령 문맥은 `bxdl setup --help`, `bxdl setup --install --help`, `bxdl setup --network --help`로 확인한다.

정상 입력은 다음 질문으로 넘어가기 전에 저장한다. Ctrl-C나 터미널·프로세스 중단 뒤에는 같은 폴더의 `--resume`으로 마지막 완료된 저장부터 이어간다. 저장이 끝나지 않은 입력은 복구됐다고 가정하지 않는다. Ctrl-C 같은 프로세스 중단을 `:cancel`의 정상 exit 5와 동일하게 취급하지 않는다.

## 검사·수정·저장

입력이 끝나면 인스턴스 이름·공개 노드 ID·주소/포트 같은 공개 설정과 자료의 선택·검사 상태를 검토한다. 데이터·chain·credential 경로는 선택됨/미입력으로 표시하며 원문 경로를 검토 요약에 표시하지 않는다. 파일 종류·권한 등 로컬 검사 결과와 다음 선택도 나온다.

| 선택 | 동작 |
| --- | --- |
| `save` 또는 Enter | 초안만 저장하고 종료 |
| `edit` | 항목 번호 하나를 선택해 수정한 뒤 검토로 복귀. 나머지 답은 유지 |
| `check` | 현재 경로의 로컬 검사 다시 수행 |
| `export` | 새 설정 JSON을 저장할 경로를 확인 |

새 설정 파일 출력과 설치·초기화·시작의 확인은 `y` 또는 `yes`만 승인한다. 빈 Enter는 거절하며 도움말·뒤로가기·취소로 실행하지 않는다.

`export`에서 경로를 생략하면 작업 폴더의 `instance.json`을 사용한다. 저장 대상 표시 후 `y` 또는 `yes`를 입력해야 내보낸다. `--output <새파일>`을 미리 주어도 대화형 확인은 유지하며, 이 옵션만으로 자동 저장하지 않는다.

기존 출력 파일은 덮어쓰지 않는다. 다른 파일 이름을 사용하거나 `--resume`으로 재개해 새 대상을 지정한다. 작업 폴더 안에서는 `instance.json`만 출력할 수 있다. 외부 출력의 부모 폴더는 이미 있어야 하며, 데이터·체인·키 자료나 초안 기록과 겹치는 출력은 거부한다.

로컬 검사가 `FAIL`이어도 형식이 유효한 초안 저장·설정 내보내기는 가능하다. 결과의 검사 항목을 확인하고 누락된 자료를 준비하거나 경로를 수정한다. 로컬 항목이 모두 통과해도 NIGO·runtime·서비스 검사가 없어 preflight는 `INCOMPLETE`다.

대화형 결과에는 저장한 입력 수, 다음 항목, 초안·설정 위치와 수정 필요·미검사 수를 한국어로 표시한다. 검사 실패에는 해당 자료와 수정 방법을 안내한다. 자동화에서 세부 검사 항목이 필요하면 비대화형 JSON 결과를 사용한다.

## 내보낸 설정으로 엔진 검사하기

기본 초안 모드에서 제품 설정을 저장한 뒤에는 같은 노드의 NIGO `node.json`과 고정한 엔진 자료를 준비하여 따로 검사한다. 기본 setup의 `check` 선택은 계속 로컬 파일 검사만 수행한다.

```bash
bxdl preflight --config ./instance.json \
  --engine-config /absolute/instance/node.json \
  --jar /absolute/package/engine/nigo-node.jar \
  --java /absolute/package/runtime/bin/java \
  --lock ./trusted-engine.lock.json --allow-development --json
```

선택한 패키지는 [install](./install.md)로 새 폴더에 설치할 수 있다. 기본 setup이 패키지를 선택·설치하거나 설치 위치를 자동 탐색하지 않는다. 다섯 엔진 옵션은 모두 함께 지정하며, 생략하면 기존 로컬 검사만 수행한다. [엔진 가이드](./engine.md)에 신뢰 lock·native 자료 준비와 제한 시간이 있다.

BXDL은 제품 설정과 native 설정의 node ID·경로·주소·포트·키 참조가 같은지 확인한다. 현재는 QBFT validator와 mTLS 구성을 명시해야 한다. native에만 있는 validator ID·peer 목록·인증 pin·동기화 설정은 [setup --network](./network.md)로 준비하거나 운영자가 직접 제공한다. 기존 초안·제품 JSON 형식은 바꾸지 않는다.

로컬 파일 문제는 exit 4로 엔진 실행 전에 멈추고, 제품/native 설정이 다르면 exit 3으로 알린다. 올바른 cold 검사도 exit 5/INCOMPLETE다. timeout은 exit 6/UNKNOWN이며 자동 재시도하지 않는다. JSON의 `data.product`, `data.configurationBinding`, 선택 `data.engine`에서 로컬 검사·설정 일치·엔진 검사를 구별할 수 있다.

엔진 cold 검사는 준비된 keystore/password·TLS 자료를 읽어 확인한다. DB 초기화·노드 시작·launchd 등록은 수행하지 않으며, 두 설정이 일치하거나 키 검사가 통과해도 실제 peer 연결과 운영 인수는 남는다.

## 기존 설정 가져오기와 자동화

`--from`은 유효한 제품 설정 JSON을 읽어 새 초안으로 가져온다. 원본 설정을 수정하지 않는다. `--resume`과 함께 사용할 수 없다.

```bash
bxdl setup --workspace "$HOME/Library/Application Support/BXDL/setup-import" \
  --from ./instance.source.json
```

자동화는 `--non-interactive`와 `--from` 또는 `--resume` 중 하나가 필요하다. `--json`은 비대화형 setup에서만 받는다. 출력할 JSON은 하나이며 질문을 출력하거나 표준입력을 읽지 않는다.

```bash
# 새 초안 저장
bxdl setup --workspace "$HOME/Library/Application Support/BXDL/setup-batch" \
  --from ./instance.source.json --non-interactive --json

# 완성된 초안을 새 설정 파일로 내보내기
bxdl setup --workspace "$HOME/Library/Application Support/BXDL/setup-batch" \
  --resume --output ./instance.new.json --non-interactive --json
```

비대화형의 `--output`은 지정한 새 파일 저장 요청이다. 별도 질문은 하지 않는다. `--output`이 없으면 초안만 저장한다. 미완성 초안을 비대화형으로 재개하면 exit 5로 남은 입력을 알리므로 대화형 `--resume`으로 이어간다.

## 경로와 저장된 자료

직접 입력한 상대경로는 **처음 setup을 시작한 디렉터리** 기준이다. 다른 디렉터리에서 재개해도 그 기준은 유지된다. 가져온 설정의 상대경로는 **원본 설정 파일의 디렉터리**에서 해석한다. 내보낸 설정은 참조를 절대경로로 저장하므로 출력 위치가 바뀌어도 같은 자료를 가리킨다. `--workspace`·`--output`의 상대경로는 각 명령을 실행한 디렉터리 기준이다.

공백 경로는 사용할 수 있다. 도우미 안에서는 `~`나 환경변수 표현을 확장하지 않으므로 실제 경로를 입력한다. symlink·자료 경로와의 겹침·의심스러운 경로 별칭은 거부할 수 있다. Mac에서는 대소문자나 유니코드 표기만 달리해 같은 위치로 해석될 가능성도 겹침으로 취급한다. 자세한 개발 조건은 [지원 범위](./support-matrix.md)를 따른다.

작업 폴더는 0700, 저장된 초안과 내보낸 설정은 0600으로 만든다. 초안은 키 내용 대신 **파일 경로를 포함한 입력값**을 보관하므로 공개 자료로 공유하지 않는다. BXDL 결과는 입력 원문·secret 참조를 되풀이하지 않지만 일반 터미널은 입력한 글자를 화면에 표시할 수 있다. 같은 사용자 권한의 프로세스로부터 초안을 격리한다고 보장하지 않는다.

완료된 저장 이력의 손상이나 동시 변경이 발견되면 오래된 상태로 자동 되돌리지 않는다. 기존 폴더·설정을 덮어쓰거나 삭제하지 말고 문제를 확인한다. 기본 setup은 원본 참조 파일의 권한을 고치거나 키·인증서·DB 내용을 열지 않으며, 새로 쓰는 범위는 초안 작업 폴더와 명시한 설정 출력이다.

## 결과 해석

| exit | setup에서의 의미 |
| --- | --- |
| 0 | 초안 저장 또는 새 설정 출력 성공. `data.preflight`의 실패·미검사 항목은 별도 확인 |
| 2 | 잘못된 옵션·조합·경로 인자 또는 대화형 터미널 부재 |
| 3 | 설정 가져오기·형식 검증·초안 읽기/저장·출력 충돌 등 실패 |
| 5 | 취소/EOF로 일시 중단하거나 비대화형 재개에 필수 입력이 남음 |
| 7 | 입력/안내·결과 출력 실패 또는 내부 오류 |

JSON의 `installation=NOT_PERFORMED`, `engineValidation=NOT_CHECKED`는 저장 성공 후에도 유지된다. 초안만 검사한 경우 설정 파일 metadata는 아직 쓰지 않았음을 나타내며, 내보내기 뒤에는 실제 출력 파일을 기준으로 metadata를 확인한다. 이 검사에도 엔진 실행은 포함되지 않는다.

setup 초안은 [Mac 우선 계획](../design/2026-09-17-rust-macos-first.md)의 R1이다. 별도 파일 installer·제품/native 결합 cold 검사와 [instance 등록·초기화·서비스 명령](./instance.md)을 제공하며, 새 설치 모드가 이 API들을 명시 선택으로 연결한다. 기본 초안 모드의 schema와 비대화형 의미는 유지한다. 전체 운영 설치 인수는 G1-M에서 따로 수행한다.
