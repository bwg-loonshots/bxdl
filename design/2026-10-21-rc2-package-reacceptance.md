# RC.2 선택과 Mac 패키지 재인수

2026-10-21 · [공급 snapshot](../contracts/nigo/engine-rc2-2026-10-21/README.md) · [인수 도구](../tests/acceptance/README.md)

## 선택과 변경 범위

사용자가 NIGO RC.2 반영과 Mac 패키지 재인수를 요청했다. source `67c9f8e5100f6ce0df7fc8271eb9d56a8ee7a36b`, JAR `b1aa84cbacf1057746bc5936308345cfdddb8f3fc78b63377af59da05dc0ce10`를 새 개발 후보로 고정한다. RC.1 lock·snapshot·실패 결과는 보존한다. 움직이는 release HEAD나 다시 빌드한 JAR로 대체하지 않는다.

계약 4개·Console·DB/schema와 선정 Temurin은 RC.1과 같다. Rust 제품 동작은 변경하지 않고 RC.2 전용 lock/spec/snapshot과 후보를 명시 선택하는 시험 도구를 추가한다. 제품 CLI는 기반 commit의 Rust source와 일치하는 release binary이며, 미커밋 인수 도구는 별도 파일 hash와 실행 사본으로 고정한다.

## 실행과 판정

새 시험 키·mTLS·거래 지갑을 준비하고 loopback의 새 포트를 사용한다. 준비 도구는 고정 공급 JAR의 시험 generator와 준비 단계의 Node/ethers만 사용한다. 준비 성공은 노드 기동이나 고객 PKI 발급을 뜻하지 않는다.

하나의 외부 시험용 Ed25519 서명 archive로 새 Mac 설치본 네 개를 만들고 설치된 CLI/Temurin/JAR의 hash를 검증한다. 제품의 네트워크 설정 도우미, 등록·명시 init, 사용자 LaunchAgent start/status/stop을 거쳐 다음을 관측한다.

1. 4/4 거래 확정과 3/4 거래 확정.
2. 뒤처진 노드의 동일 DB 재시작·canonical header/state catch-up, 재합류 후 네 노드의 새 거래 확정.
3. 2/4 상태 45초 동안 미확정·라운드 증가, 3/4 복귀 뒤 원 거래의 재제출 없는 확정, 네 노드 복귀 후 후속 거래.
4. 별도 2/4 상태 45초로 라운드 차이를 만들고 네 노드를 모두 정상 종료. 동일 네 DB·키·제품/native 설정으로 재시작하고 원 signed transaction을 명시적으로 한 번 재제출. 네 노드 확정 및 후속 거래를 확인한다.
5. 잘못된 peer expected pin의 outbound WRONG_PIN·peer 0과 나머지 정상 세 노드의 거래 확정.

새 거래는 모든 지정 노드에서 receipt hash/type/status/height, block 내 거래 포함, 동일 block hash/state root를 요구한다. snapshot 이전 과거 receipt 복원은 요구하지 않으며 존재 여부를 따로 남긴다. UNKNOWN은 실패/미완료 근거로 보존하고 자동 재전송이나 DB/WAL/round 삭제로 우회하지 않는다. 명시 재제출은 4번에만 한정한다.

기본 full 시나리오와 별도로 새 fixture/출력에서 same-db-recovery만 실행할 수 있다. 앞선 실패를 다른 실행의 성공으로 덮어쓰지 않는다. 결과는 실행별 package·CLI·JAR·JRE·fixture·하네스 hash, 프로필, 관측과 정상 정리 근거로 기록한다. 모든 소유 서비스의 정상 STOPPED·잠금 해제와 잔존 프로세스/포트 여부를 확인한다.

## 완료와 남는 범위

필수 검사는 `make check`, 인수 도구의 후보 혼용·중복 제출·판정/정리 회귀, 실제 Mac package 실행이다. 실제 결과는 별도 `results/2026-10-21-macos-rc2-package.md`와 JSON에 기록하며 이 설계서는 PASS를 선언하지 않는다.

RC.2로 새로 만든 설치의 동일 DB 재시작과 RC.1 기존 설치를 RC.2로 전환하는 upgrade는 다르다. 현재 제품은 기존 등록의 package/lock/binding 변경이나 비어 있지 않은 data의 새 등록을 허용하지 않는다. 따라서 RC.1→RC.2 제품 upgrade는 미구현·미시험으로 남기며 니고의 엔진 단독 forward 복구를 제품 UX PASS로 옮기지 않는다.

전체 G1-M, 로그인/로그아웃·sleep/wake·crash/power-cut, 안전 제거·upgrade, 다중 호스트·Linux/systemd·Docker, 장기/성능 검증과 J5 공급망/고객 재배포 승인은 별도다. 니고 원 저장소나 기존 서비스/데이터는 변경하지 않는다.
