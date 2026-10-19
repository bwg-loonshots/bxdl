# 선정 JRE의 NIGO 회신 수신·검토

2026-10-19 · [수신 원장](./receipt.json) · [NIGO PR #212](https://github.com/bwg-loonshots/nigo-protocol/pull/212).
NIGO 문서 revision `e3f55ecb5eb19ee9dc32737687902836edb62e27`의 J1~J3 회신을 지정 개발 조합의 근거로 수용한다.
기존 RC1·Temurin lock을 유지한다. 이 기록은 회신 코드·기존 원자료를 대조한 결과이며 새로운 공급자 시험 실행 결과가 아니다.

## 수용 범위

- J1: Java21·full JRE 유지, management 모듈·TLS1.3·NGVK·native/임시 경로 조건을 수용한다. 추가 필수 JVM 옵션은 지정되지 않았다. 최소 모듈·최소 heap·최소 OS는 미확정이다.
- J2: 외부 JAR의 재빌드 차단, 지정 Java 전파, 원본/복사 JAR와 전체 runtime inventory의 전후 검증, 정확한 9사례·실제 자식 실행과 종료를 확인하는 하네스를 수용한다.
- J3: clean 하네스 `6489c954`에서 고정 RC1/Temurin의 9/9 PASS를 수용한다. H2/RocksDB lifecycle·4-validator+observer·동일 identity 재시작, mTLS·native·runtime 관측의 공급자 근거다.

공급자 run-result/environment 원문 hash를 고정하고, 기록된 runtime inventory를 BXDL runtime lock과 대조했다. runtime 259개 파일과 모듈49개가 일치했다.
66개 자식 JVM·18개 HTTP runtime/native probe·10개 mTLS probe·72개 native 호출의 근거를 확인했다.
공급자 snapshot identity와 원문 URL/hash는 수신 원장에 고정한다. key·DB·원본 전체 실행 로그는 포함하지 않는다.

## BXDL에 적용

RocksDB 5대 동시 기동의 16.113~16.775초는 `OPENING_MANAGED_STORAGE`부터 `RUNNING`까지의 context 초기화 시간이다.
최초 HTTP READY 또는 합의 준비 SLA가 아니다. 기존 BXDL 15초 관측의 INCOMPLETE와 같은 실행의 후속 READY/정상 종료는 과거 기록으로 유지한다.
이후 [시작 관측 UX](../../../design/2026-10-19-start-observation-ux.md)에서 기한 종료·실제 실패를 구분하고 같은 시도를 읽기 관측한다.

J4의 실제 BXDL package/LaunchAgent/4-validator·전체 G1-M, J5의 공급자 SBOM 불일치·전체 NOTICE/소스 공급·출처 검증은 남아 있다.
기존 RC1을 새 main의 Native 기능이나 H2 공식 공급 범위로 확대하지 않으며, REQ-0003 전체 완료·고객 출시를 선언하지 않는다.
