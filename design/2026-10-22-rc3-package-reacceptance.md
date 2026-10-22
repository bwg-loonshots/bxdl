# RC.3 선택과 Mac 패키지 재인수

2026-10-22 · [수신 snapshot](../contracts/nigo/engine-rc3-2026-10-22/README.md) · [인수 도구](../tests/acceptance/README.md) ·
[등록 자료 보관·시작 진단](./2026-10-22-managed-supply-and-start-diagnostics.md)

## 명시 선택과 이전 기록

사용자 요청에 따라 기존 변경을 [BXDL PR #13](https://github.com/bwg-loonshots/bxdl/pull/13)으로 먼저 머지한 뒤,
RC.3를 새로운 Mac 개발 package 재인수 대상으로 명시 선택한다. BXDL 제품 source 기반은
`53bb11b90d9d51aba5a7b509074046fd6ee78401`이며 이번 후보 전환 때문에 Rust 제품 source를 변경하지 않는다.
움직이는 NIGO main/release HEAD나 재빌드한 JAR로 고정 후보를 대신하지 않는다.

앞선 수신 snapshot의 `explicitCandidateAdoption=NOT_PERFORMED`, package/A7 `NOT_RUN`은 **그 수신 시점의 사실**로
그대로 둔다. 이번 선택은 이 설계와 새 lock/spec으로 구분하며 실제 실행 결과는 별도 결과 기록에 연결한다.
RC.1/RC.2 lock·snapshot·기존 PASS/INCOMPLETE와 공급자의 원본 manifest 상태도 수정하지 않는다.

## 고정 입력과 변경 필드

| 항목 | 선택한 값 |
| --- | --- |
| NIGO 인계 문서 revision | `75c2666c2719ff39499f4772821f71d0a425d90e` |
| 공급 태그 / source | `nigo-engine-v0.1.0-rc.3` / `7ec5abd8c98e1cf070763a16f815e9281b74a5c1`, dirty=false |
| JAR SHA-256 / 크기 | `05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f` / 164,088,530 bytes |
| 공급 archive SHA-256 | `c080b0366a43b755868d359533712bb64122187d361497689501c7a5910c02a3` |
| 계약 fingerprint | `c604b1d24097015b2839d97f2bc9d45e3fff91ed6984665d1aa827a155c7b6e8` |
| Console fingerprint | `a76e634dacf728b81cd446cdeaa2b6a250e1a2a91c505153dd424fd46431e99e` |
| runtime | Temurin21.0.12.1+1-LTS full JRE, macOS arm64·Java21·259파일·49모듈 |
| Java SHA-256 | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |
| runtime inventory 원문 SHA-256 | `fb779d849cdb9153b694b3ad368dc64b178af71a118d148dcd4d51a3e7853761` |

- [RC.3 engine lock](../packaging/engine-lock.rc3-temurin-development.json)의 `expected`는 수신 당시 실제 실행한
  `engine-info`의 전체 JSON identity와 같다. RC.2 대비 JAR hash/크기, source commit, 계약 revision·문서 hash·정의 hash·
  fingerprint, Console fingerprint의 8개 필드가 바뀐다. Java와 runtime 계약/fixture hash는 동일하다.
- [RC.3 package spec](../packaging/package-spec.rc3-temurin.example.json)은 engine revision·JAR hash·계약 fingerprint를
  바꾼다. `engine.contractRevision`은 source commit이 아니라 **contract fingerprint**다. 예시의 `files=[]`와
  제품 revision `development`는 실제 package 검증 근거가 아니다. 조립 시 실행 CLI·JAR·JRE·문서 전체 inventory와
  실제 제품 revision을 결속하고 새 archive/manifest hash를 기록한다.

RC.3는 공식 `storage-integrity` 명령·관련 종료 코드와 Console Vite8.2.4 패치를 포함한다. protocol/DB schema/profile
revision을 올리지 않는다. 공급 범위는 `development`, `officialRelease=false`, 계약은 `PROPOSED`다.
공급자의 CLI/Storage/복구 PASS는 참고 근거이며 아래 소비자 실행의 성공을 대신하지 않는다.

## 하나의 package로 수행할 Mac 인수

시험 도구는 `--candidate rc3`를 명시하고 source/JAR hash·크기·Java·lock·fixture가 다른 후보와 섞이면 거부한다.
실행 전 현재 하네스를 복사·파일별 hash로 고정한다. 기존 RC.1의 제한·RC.2 결과를 보존하면서 RC.3도 두 복구 경로를
수행하도록 후보 제한을 갱신한다. source checkout/Gradle 재빌드 없이 받은 JAR를 사용한다.

새 시험 키·mTLS·거래 fixture와 fresh data를 준비하고, 현재 CLI와 고정 RC.3/Temurin을 담은 **한 개의 외부 시험 key 서명
archive**를 만든다. 설치마다 그 archive/manifest·CLI·JAR·runtime 전체 inventory를 확인한다. 고객 키·DB·등록은 사용하지 않는다.

첫째, 같은 archive의 새4-validator 설치본에서 제품 네트워크 도우미·V2 등록·명시 init·사용자 LaunchAgent를 거친다.

1. 4/4와3/4 거래 확정, 뒤처진 노드의 같은 DB canonical header/state catch-up과 재합류 후 새 거래.
2. 2/4 정족수 상실45초의 미확정/라운드 증가, 3/4 복귀 뒤 원 거래 **재제출 없는** 확정, 네 노드 복귀 후 새 거래.
3. 독립2/4 상실45초로 round skew를 만든 뒤 전체 정상 종료. 같은 네 DB·키·설정으로 새 attempt를 시작하고
   원 signed transaction을 **명시1회 재제출**하여 네 노드 확정과 후속 거래를 확인한다.
4. 잘못된 expected pin의 outbound WRONG_PIN·peer0, 정상 세 노드의 새 거래 확정, 모든 소유 서비스 정상 종료.

새 거래는 지정 노드 모두의 receipt hash/type/status/height·블록 내 거래 포함·동일 block hash/state root를 요구한다.
과거 snapshot catch-up은 header/height/state 계약으로 판정하고 과거 receipt/body 복원을 요구하지 않는다.
UNKNOWN·실패는 보존하며 자동 재전송·강제 종료·DB/WAL/round 삭제로 통과시키지 않는다.

둘째, 위 **동일 archive**로 별도 새 setup/control/data를 만들고, 합의 자동 시작·동기화를 끈 단일 노드 로컬 profile에서
[앞선 자료 보관 시험](../results/2026-10-22-macos-managed-supply.md)의13개 흐름을 RC.3로 다시 확인한다.
등록 완료 후 test-owned 원본 archive·공개키·lock 경로를 옮겨 원본이 없는 상태를 만들되 원본 bytes는 별도 보존한다.
setup 재개/check·cold의 MATCHED/INCOMPLETE, 명시 init/start·READY·7단계 진입·diagnose,
정상 stop·같은 DB 재시작·다시 정상 stop과 보관본/운용 입력 불변을 확인한다. 기존 등록의 binding을 고치지 않는다.
4-validator 서비스와 동일 서명 identity의 시험 노드를 동시에 실행하지 않는다.

두 실행의 종료는 정상 STOPPED·잠금 해제, 소유 PID 부재·LaunchAgent NOT_LOADED·OS TCP LISTEN0을 각각 확인한다.
bind 성공/실패는 LISTEN 판정 대신 사용하지 않으며 관측 오류·도구 부재는 UNKNOWN이다. 입력·설치 payload와 runtime을
종료 후 다시 대조하고 원본 결과/하네스는 변경하지 않는다. 과거 A/B INCOMPLETE를 새 성공으로 덮어쓰지 않는다.

## 완료 판정과 별도 범위

`make check`의 Rust363개·Python50개·fmt·Clippy와 Mac release build가 PASS다. 후보 혼용·중복 제출·두 복구 경로·
종료 관측 회귀를 포함한다. 실제 실행의 제품 revision/CLI·archive·JAR·JRE·하네스·fixture identity와 사례별 결과는
[4-validator 패키지 재인수](../results/2026-10-22-macos-rc3-package.md)와
[같은 archive의 setup 재인수](../results/2026-10-22-macos-rc3-setup.md)에 기록한다.

**지정 범위의 두 실행과 독립 검사가 PASS**다. 4-validator는 거래8개·제출9회, 두45초 정족수 복구와 wrong-pin,
정상 stop12회·기록된62 PID 잔존0·12개 job NOT_LOADED·8포트 LISTEN0을 확인했다. 5개 설치본의 각270파일과
V2 보관 자료를 대조했다. 같은 archive의 별도 setup13사례에서는 원본 공급 세 경로 부재 후 재개·초기화·운용과
정상 stop2회·기록된14 PID 잔존0·2개 job NOT_LOADED·2포트 LISTEN0을 확인했다.
이번 setup은 제품 설정을 가져온 뒤 공급 자료 문답을 진행했으며 전체14문답·cancel/hangup은 재실행하지 않았다.
원 수신 snapshot의 당시 미선택·NOT_RUN과 기존 RC.1/RC.2 결과는 그대로 유지한다.

**offline Storage A7은 별도**다. 이번 범위에서 `storage-integrity inspect/repair`나 고객 DB의 복구를 실행하지 않는다.
A7에는 서비스 중지·자동 재기동 차단·backend/경로 매핑·백업·명시 repair 확인·결과 불명 보존과 독립 재검사/재시작이 필요하다.
공급자의 같은DB RC.1→RC.3 JAR-only forward 결과도 BXDL 기존 설치 upgrade 인수가 아니다.

기존 V1 등록의 변환·제품 upgrade/downgrade, 보호 폴더/TCC 원인 규명, 로그아웃·sleep/wake·crash/power-cut·안전 제거,
전체 G1-M·J5/고객 재배포·Native kit 전체 기능·다중 호스트·Linux/systemd·Docker·장기/성능은 별도다.
NIGO 자체 배포 조건 미정·과거 비용 gate 직접 원인 `UNRESOLVED`를 유지하고 NIGO 저장소나 기존 서비스/데이터를 변경하지 않는다.
